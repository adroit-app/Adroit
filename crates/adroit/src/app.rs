use std::cell::{Cell, RefCell};
use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use adroit_agents::claude::commands;
use adroit_model::Session;
use adroit_nvim::{Embedded, Event as NvimEvent, notation};
use adroit_term::{Key, Palette, Pane, Screen, Size, Spawn};
use adroit_vcs::Repo;
use dioxus::prelude::*;
use tokio::sync::{Notify, mpsc, oneshot};

use crate::actions::Action;
use crate::bridge::{AppCtx, use_app};
use crate::bundled_font::{faces, use_bundled_font};
use crate::complete::{self, candidates};
use crate::completion::Completions;
use crate::console::{Composer, ConsoleScreen, Hub, Panes, ReaderScreen, mode_of};
use crate::console_read;
use crate::content::{
    Area, Axis, ContentPanel, DraftForm, NothingOpen, Opening, Starting, Tab, TerminalAsk,
    WindowId, draft as draft_agent, file_name, fresh_name, label_from, named, shows_prompt,
};
use crate::daemon;
use crate::details::{self, Entry, Sheet, Shot, Source};
use crate::editor;
use crate::image_window;
use crate::keymap::{Chord, Context, Keymap, MENU_KEYS};
use crate::keys;
use crate::layout::Side;
use crate::menu_bar;
use crate::nav::{self, Item};
use crate::pasted;
use crate::paths::{home, instance_state, keys_path, scratch_dir};
use crate::picker::Picker;
use crate::pictures::{self, Gallery};
use crate::programs;
use crate::prompt_tab::{PromptTab, TabGoes};
use crate::prompts::{self, Op, Prompts};
use crate::rename::{Place as RenamePlace, Rename};
use crate::scratch::{self, Moment};
use crate::sessions;
use crate::settings;
use crate::settings_window;
use crate::shell::overlays::{ConfirmDialog, ContextMenu, DaemonBanner, KeysHelp, old_daemon_note};
use crate::shell::{self, header::Header, panels::Splitter, status_bar::StatusBar};
use crate::standing;
use crate::style::{CSS, CellHeight, DEFAULT_CELL_HEIGHT};
use crate::themes;
use crate::tree::{RowContext, RowEvents, TreePanel};
use crate::view::Keys;
use crate::viewers;
use crate::window_socket::{Request, Response, ask, bind, serve, socket_path};
use crate::work_host;
use crate::zoom;
use adroit_agents as agents;
use adroit_core::console::reader::{self, Edge};
use adroit_core::console::route::{self, Footing, Typed, passes_enter};
use adroit_core::console::{Probe, ReaderAt};
use adroit_core::effect::Effect as CoreEffect;
use adroit_core::msg::Msg;
use adroit_core::overlay;
use adroit_core::overlay::confirm::Ask;
use adroit_core::overlay::context_menu::Target;
use adroit_core::tree as tree_view;

fn shows_usage(tab: Option<&Tab>, agent: adroit_model::Agent) -> bool {
    matches!(tab, Some(Tab::Session(_) | Tab::Draft { .. }))
        && agents::adapter(agent).capabilities().usage_limits
}

/// The palette of the theme the settings name, or adroit's own when it
/// cannot be read.
fn initial_palette() -> Palette {
    themes::palette(&settings::current().theme, &crate::settings::themes_dir()).unwrap_or_default()
}

/// Open the window.
pub fn launch() {
    use dioxus::desktop::{Config as Desktop, LogicalSize, WindowBuilder};
    settings::watch(Duration::from_secs(1));
    let cfg = Desktop::new()
        .with_window(
            WindowBuilder::new()
                .with_title(adroit_core::chrome::title(
                    crate::standing::DESCRIBED.filter(|_| cfg!(debug_assertions)),
                ))
                .with_inner_size(LogicalSize::new(1440.0, 900.0)),
        )
        .with_menu(menu_bar::build())
        .with_background_color((15, 17, 20, 255))
        .with_custom_head(format!("<style>{}{CSS}</style>", faces()));
    dioxus::LaunchBuilder::desktop().with_cfg(cfg).launch(App);
}

/// Whether this window answers on the socket, and so takes Claude Code's
/// input through its external editor.
static ANSWERS: AtomicBool = AtomicBool::new(false);

/// The least time between showing the console's reader what the console
/// holds and showing it again, while the console keeps changing: each
/// showing hands Neovim the whole console.
const READER_GAP: Duration = Duration::from_millis(30);

/// The program that puts its input on the system clipboard.
fn clipboard_command() -> std::process::Command {
    let mut cmd = std::process::Command::new("pbcopy");
    // pbcopy decodes its input by the locale. An app opened from Finder has
    // none, so pbcopy falls back to the user's legacy text encoding (Mac
    // Japanese, say), fails on UTF-8 and empties the clipboard without an
    // error. `LC_CTYPE` alone loses to an `LC_ALL` the app may inherit.
    cmd.env("LC_ALL", "en_US.UTF-8");
    cmd
}

/// Put `text` on the system clipboard.
pub(crate) fn copy_to_clipboard(text: &str) -> std::io::Result<()> {
    use std::io::Write;
    let mut child = clipboard_command()
        .stdin(std::process::Stdio::piped())
        .spawn()?;
    child
        .stdin
        .take()
        .expect("stdin is piped")
        .write_all(text.as_bytes())?;
    child.wait()?;
    Ok(())
}

/// How long a session is given to repaint after Tab before its input is
/// asked for anyway.
const TAB_APPLIED_WAIT: Duration = Duration::from_millis(500);

/// Wait until `pane` shows something other than `before`, or
/// [`TAB_APPLIED_WAIT`] passes.
async fn repainted(pane: &Pane, before: &Screen) {
    let until = tokio::time::Instant::now() + TAB_APPLIED_WAIT;
    while tokio::time::Instant::now() < until && pane.screen() == *before {
        tokio::time::sleep(Duration::from_millis(16)).await;
    }
}

/// Show `shot` of the overview in a window of its own. A drawn image is
/// found among those `gallery` keeps for `session`; a file too large to read
/// here is shown by macOS's own viewer.
fn show_shot(shot: &Shot, session: &str, gallery: &Gallery) {
    match &shot.source {
        Source::Console { id, width, height } => {
            let Some(kept) = gallery.store.find(session, *id) else {
                return;
            };
            image_window::open(
                &shot.key,
                &format!("Image {width} × {height}"),
                pictures::kept_shown(kept),
                Some((*width, *height)),
            );
        }
        Source::File { path, .. } => {
            let (key, path) = (shot.key.clone(), PathBuf::from(path));
            spawn(async move {
                let read = path.clone();
                let shown = tokio::task::spawn_blocking(move || pictures::file_shown(&read))
                    .await
                    .ok()
                    .flatten();
                match shown {
                    Some(src) => {
                        let title = path.file_name().map_or_else(
                            || path.display().to_string(),
                            |n| n.to_string_lossy().into_owned(),
                        );
                        image_window::open(&key, &title, src, None);
                    }
                    None => crate::bridge::open([path.display().to_string()]),
                }
            });
        }
    }
}

/// Keep the images `drawn` in `session`'s console, encoded away from the
/// window's thread.
fn keep_drawn(mut gallery: Signal<Gallery>, session: String, drawn: Vec<adroit_term::Drawn>) {
    // A program may send the same image again as it redraws; it is kept
    // already, and encoding it again would be for nothing.
    let drawn: Vec<_> = {
        let g = gallery.peek();
        drawn
            .into_iter()
            .filter(|d| g.store.find(&session, pictures::id_of(d)).is_none())
            .collect()
    };
    if drawn.is_empty() {
        return;
    }
    spawn(async move {
        let at = now_ms();
        let kept = tokio::task::spawn_blocking(move || {
            drawn
                .into_iter()
                .filter_map(|d| pictures::Kept::of(d, at))
                .collect::<Vec<_>>()
        })
        .await
        .unwrap_or_default();
        if kept.is_empty() {
            return;
        }
        let mut g = gallery.write();
        for k in kept {
            g.store.keep(&session, k);
        }
    });
}

/// How long the prompt's Neovim has to live to be started again once it
/// quits.
const NVIM_SETTLE: Duration = Duration::from_secs(2);

/// The console screen's top padding.
const SCREEN_TOP: f64 = 10.0;

/// The console's height kept clear besides the prompt card: the screen's top
/// padding and the card's margin below and above.
const CONSOLE_GAP: f64 = SCREEN_TOP + 14.0 + 8.0;

/// The console's height left to its terminal, with the prompt card of this
/// height under it, if one is shown.
fn console_room(height: f64, card: Option<f64>) -> f64 {
    match card {
        Some(card) => height - card - CONSOLE_GAP,
        None => height - SCREEN_TOP,
    }
}

/// How long, in 50 ms steps, a prompt waits for a session pane that has just
/// attached.
const ATTACH_WAIT_STEPS: usize = 60;

/// How long the window waits before it runs [`INPUT`] again, once the keys
/// it listened for are lost.
const RESTART_INPUT_AFTER: Duration = Duration::from_millis(200);

/// How often the window looks whether its page has its keys.
const LOOK_FOR_KEYS_EVERY: Duration = Duration::from_millis(500);

/// Keeps the hidden text area focused, but while a text field has the keys,
/// and turns what it receives into messages. Text is taken from
/// `beforeinput` (from `keydown` while the text area is read-only), pastes
/// from `paste` and
/// input-method commits from `compositionend`, each stopped in JavaScript
/// before the text area changes: a default prevented from Rust arrives too late
/// once the event has crossed to the webview, and the stray character then
/// lands out of order. Other keys have their default stopped here for the same
/// reason; Rust reads them from `keydown`. Command keys the menu bar holds keep
/// their default, so the menu runs them, as it does copy, paste and quit:
/// `window.adroitHeld` lists them as [`menu_bar::left_to_menu`] names them.
const INPUT: &str = include_str!("../assets/js/input.js");

/// The width and height of one cell, measured with the console's own font.
const MEASURE: &str = include_str!("../assets/js/measure.js");

fn nvim_program() -> String {
    std::env::var("ADROIT_NVIM").unwrap_or_else(|_| "nvim".into())
}

/// This program, as a session should call it to reach the window.
fn adroit_program() -> PathBuf {
    std::env::current_exe().unwrap_or_else(|_| "adroit".into())
}

/// Now, in milliseconds since the Unix epoch.
fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_millis() as u64)
}

fn short(path: &Path) -> String {
    adroit_core::picker::short(path, &home())
}

/// Start writing in the prompt, whatever mode it was left in: a draft asks
/// for its first prompt.
fn start_writing(nvim: Signal<Option<Arc<Embedded>>>) {
    if let Some(nv) = nvim.peek().clone() {
        spawn(async move {
            let _ = nv.start_writing().await;
        });
    }
}

/// Close a shell's windows, and show another thing of its worktree if no
/// window is left.
fn close_shell(id: u128, panes: &Panes, app: AppCtx) {
    panes.borrow_mut().remove(&Tab::Terminal(id));
    app.dispatch(Msg::ShellClosed(id));
}

/// End a shell, as closing its tab does.
fn end_shell(hub: &Hub, id: u128) {
    let daemon = hub.daemon.clone();
    spawn(async move {
        let _ = daemon.end(id).await;
    });
}

/// Move the text a session handed its external editor in `path` into the
/// prompt, leaving the file empty so the session's input is cleared. A file
/// adroit did not ask for is left alone: the user opened the editor.
async fn take_input(path: &Path, prompts: Prompts, app: AppCtx) -> Response {
    let asked = app.console.peek().pull.clone();
    let Some(tab) = asked else {
        return Response::Declined;
    };
    app.dispatch(Msg::InputTaken);
    let lines = editor::typed(&std::fs::read_to_string(path).unwrap_or_default());
    if lines.is_empty() {
        return Response::Taken(0);
    }
    // The input is emptied only once the prompt holds the text, or keeps it
    // for the tab it was asked for; with no prompt to take it, Claude Code
    // keeps it.
    let (took, taken) = oneshot::channel();
    if prompts
        .send(Op::Add(Some(tab), lines.clone(), Some(took)))
        .is_err()
        || !taken.await.unwrap_or(false)
    {
        return Response::Taken(0);
    }
    if let Err(e) = std::fs::write(path, "") {
        app.status(Some(format!("cannot clear the session's input: {e}")));
        return Response::Taken(0);
    }
    let n = lines.len();
    app.notice(moved_notice(n));
    Response::Taken(n)
}

fn moved_notice(n: usize) -> String {
    format!(
        "moved {n} line{} from the session's input to the prompt",
        if n == 1 { "" } else { "s" }
    )
}

/// The daemon's id of the session a tab shows; `None` for a draft and for a
/// session running in another terminal.
fn kept_id(tab: &Tab) -> Option<u128> {
    match tab {
        Tab::Session(id) => sessions::id_of(id),
        Tab::Terminal(id) => Some(*id),
        Tab::Draft { .. } | Tab::Viewer(_) => None,
    }
}

/// The pane a tab shows, attaching to its session when there is none yet:
/// `None` until the attach lands, when the tab is woken and `attached` told.
fn ensure_pane(
    app: AppCtx,
    hub: &Hub,
    tab: &Tab,
    size: Size,
    attached: impl FnOnce() + 'static,
) -> Option<Arc<Pane>> {
    if let Some(p) = hub.panes.borrow().get(tab)
        && !p.exited()
    {
        return Some(p.clone());
    }
    let id = kept_id(tab)?;
    if !hub.attaching.borrow_mut().insert(tab.clone()) {
        return None;
    }
    tell_live(app, hub);
    let hub = hub.clone();
    let tab = tab.clone();
    spawn(async move {
        let wake = hub.wake.clone();
        let me = tab.clone();
        let palette = hub.palette.peek().clone();
        let pane = Pane::attach(&hub.daemon, id, size, &palette, move || {
            let _ = wake.send(me.clone());
        })
        .await;
        hub.attaching.borrow_mut().remove(&tab);
        if let Ok(pane) = pane {
            hub.panes.borrow_mut().insert(tab.clone(), Arc::new(pane));
            attached();
            let _ = hub.wake.send(tab);
        }
        tell_live(app, &hub);
    });
    None
}

/// Start a session as `spawn` says, with the pane of the tab `tab_of` its id
/// names, so that its program starts in the theme's colours.
async fn start(
    app: AppCtx,
    hub: &Hub,
    spawn: Spawn,
    size: Size,
    tab_of: impl Fn(u128) -> Tab,
    recorded: Option<crate::delegation::app::Recorded>,
) -> adroit_term::Result<u128> {
    let palette = hub.palette.peek().clone();
    let wake = hub.wake.clone();
    let failure_owner = recorded.clone();
    let pane = Pane::start_prepared(
        &hub.daemon,
        spawn,
        size,
        &palette,
        |id| {
            let tab = tab_of(id);
            move || {
                let _ = wake.send(tab.clone());
            }
        },
        move |id| async move {
            match recorded {
                Some(recorded) => recorded.attach(id).await,
                None => Ok(()),
            }
        },
    )
    .await;
    let pane = match pane {
        Ok(pane) => pane,
        Err(error) => {
            if let Some(owner) = failure_owner {
                owner.failed().await;
            }
            return Err(error);
        }
    };
    // A theme chosen while the session was being started passed it by.
    let now = hub.palette.peek().clone();
    if now != palette {
        pane.set_palette(&now);
    }
    let id = pane.id();
    hub.panes.borrow_mut().insert(tab_of(id), Arc::new(pane));
    tell_live(app, hub);
    Ok(id)
}

/// The sessions shown live here, or being attached to.
fn live_sessions(hub: &Hub) -> BTreeSet<String> {
    let panes = hub.panes.borrow();
    let attaching = hub.attaching.borrow();
    panes
        .iter()
        .filter(|(_, p)| !p.exited())
        .map(|(t, _)| t)
        .chain(attaching.iter())
        .filter_map(|t| match t {
            Tab::Session(id) => Some(id.clone()),
            _ => None,
        })
        .collect()
}

/// Tell the view state which sessions are shown live here: one started a
/// moment ago may not be listed yet, and is not taken for ended.
fn tell_live(app: AppCtx, hub: &Hub) {
    let live = live_sessions(hub);
    app.dispatch(Msg::PanesLive(live));
}

/// The pane a tab shows, waiting a while for its attach when there is none.
async fn pane_for(app: AppCtx, hub: &Hub, tab: &Tab, size: Size) -> Option<Arc<Pane>> {
    for _ in 0..ATTACH_WAIT_STEPS {
        if let Some(p) = ensure_pane(app, hub, tab, size, || {}) {
            return Some(p);
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    None
}

#[component]
fn App() -> Element {
    use_bundled_font();
    let app = use_app();
    // The sessions and terminals as the work host last listed them.
    let listed = use_memo(move || {
        app.host
            .read()
            .work
            .as_ref()
            .and_then(|w| w.tree.listed.clone())
    });
    let tree = use_memo(move || {
        listed
            .read()
            .as_ref()
            .map(|l| l.tree.clone())
            .unwrap_or_default()
    });
    let terminals = use_memo(move || {
        listed
            .read()
            .as_ref()
            .map(|l| l.terminals.clone())
            .unwrap_or_default()
    });
    // The latest action for a viewer, with the window whose viewer it is for.
    let mut viewer_command = use_signal(|| None::<(WindowId, viewers::Command)>);
    // The content as the view state holds it, each part read alone.
    let active = use_memo(move || app.content.read().active().cloned());
    let focus = use_memo(move || app.content.read().focus);
    let place = use_memo(move || app.content.read().place.clone());
    let mut window_focused = use_signal(|| dioxus::desktop::window().is_focused());
    let mut completions = use_signal(Completions::default);
    use_effect(move || {
        let tab = active();
        let shown = match (&tab, window_focused()) {
            (Some(Tab::Session(id)), true) => Some(id.as_str()),
            _ => None,
        };
        let next = completions.peek().updated(&tree.read(), shown);
        if *completions.peek() != next {
            completions.set(next);
        }
    });
    let mut frame = use_signal(|| 0u64);
    let mut prompt_frame = use_signal(|| 0u64);
    let mut prompt_mode = use_signal(|| ("normal", "…"));
    // Moves when a pane is started, for what the console says around it.
    let mut attached = use_signal(|| 0u64);
    let mut prompt_rows = use_signal(|| settings::current().prompt_min_rows);
    // The lines Neovim asks for, which the settings bound.
    let mut prompt_height = use_signal(|| 1usize);
    // The line the prompt shows from, and how many it has.
    let mut prompt_view = use_signal(|| (0usize, 0usize));
    // Whether the prompt's text starts with `!`.
    let mut prompt_shell = use_signal(|| false);
    let mut prompt_cols = use_signal(|| 80u16);
    let mut cell_width = use_signal(|| 7.8f64);
    let mut console_size = use_signal(|| Size::new(100, 30));
    let mut console_box = use_signal(|| (0.0f64, 0.0f64));
    let composer_height = use_signal(|| 0.0f64);
    let prompt_width = use_signal(|| 0.0f64);
    let mut nvim = use_signal(|| None::<Arc<Embedded>>);
    // The console read in a Neovim of its own, for Vim's moves, search,
    // selection and yank, while the console says it is read.
    let mut reader = use_signal(|| None::<Arc<Embedded>>);
    let mut reader_frame = use_signal(|| 0u64);
    // The line and column the reader shows from, which images follow.
    let mut reader_view = use_signal(|| (0usize, 0usize));
    // How many lines the reader holds, and how many of them are the screen,
    // the last.
    let mut read_rows = use_signal(|| (0usize, 0usize));
    // The zoom alone, which the console is measured again for.
    let zoomed = use_memo(move || app.chrome.read().zoom);
    // The settings the console's font is drawn by, and the height of a line
    // measured in it.
    let mut font = use_signal(settings::current);
    let mut cell_height_now = use_signal(|| DEFAULT_CELL_HEIGHT);
    use_context_provider(|| CellHeight(cell_height_now));
    // The theme's palette, which a new theme replaces.
    let palette = use_signal(initial_palette);
    // The side panels' widths and whether they are shown, as the chrome
    // holds them.
    let layout = use_memo(move || app.chrome.read().panels.layout);
    // How large each window of the content that is not focused is drawn,
    // whose program is sized to it.
    let win_boxes = use_signal(HashMap::<WindowId, (f64, f64)>::new);
    let toggle_panel = move |side: Side| app.dispatch(Msg::PanelToggle(side));
    // The tabs with something written for them kept aside.
    let drafted = use_signal(HashSet::<Tab>::new);
    let shown_images = use_signal(Vec::<PathBuf>::new);
    let mut selected_image = use_signal(|| None::<PathBuf>);
    let prompts = use_hook(|| prompts::keeper(nvim, drafted, shown_images));
    use_effect(move || {
        let has_images = !shown_images.read().is_empty();
        selected_image.set(None);
        let s = settings::current();
        let rows = crate::console::prompt_rows(
            *prompt_height.peek(),
            has_images,
            s.prompt_min_rows,
            s.prompt_max_rows,
        );
        if rows != *prompt_rows.peek() {
            prompt_rows.set(rows);
            if let Some(nv) = nvim.peek().clone() {
                let cols = *prompt_cols.peek();
                spawn(async move {
                    let _ = nv.resize(cols, rows as u16).await;
                });
            }
        }
    });
    // The key bindings in force, as the chrome holds them.
    let keymap = use_memo(move || app.chrome.read().keymap.clone());
    // The images the consoles drew.
    let gallery = use_signal(Gallery::default);

    let hub = use_hook(|| {
        let (tx, mut rx) = mpsc::unbounded_channel::<Tab>();
        let panes: Panes = Rc::new(RefCell::new(HashMap::new()));
        // Coalesce wakeups into at most one redraw a frame, and only for the
        // panes on screen; another pane is drawn afresh when it is shown. A
        // shell that has exited has its windows closed.
        let watched = panes.clone();
        spawn(async move {
            while let Some(first) = rx.recv().await {
                tokio::time::sleep(Duration::from_millis(16)).await;
                let mut woke = vec![first];
                while let Ok(t) = rx.try_recv() {
                    woke.push(t);
                }
                for tab in &woke {
                    if let Tab::Session(id) = tab {
                        let drawn = watched
                            .borrow()
                            .get(tab)
                            .map(|p| p.take_drawn())
                            .unwrap_or_default();
                        if !drawn.is_empty() {
                            keep_drawn(gallery, id.clone(), drawn);
                        }
                    }
                    let gone = watched.borrow().get(tab).is_some_and(|p| p.exited());
                    if let (Tab::Terminal(id), true) = (tab, gone) {
                        close_shell(*id, &watched, app);
                    }
                }
                let on_screen = app
                    .content
                    .peek()
                    .windows
                    .current()
                    .is_some_and(|t| t.windows().iter().any(|w| woke.contains(&w.shows)));
                if on_screen {
                    *frame.write() += 1;
                }
            }
        });
        Hub {
            daemon: daemon::daemon(),
            panes,
            attaching: Rc::new(RefCell::new(HashSet::new())),
            wake: tx,
            palette,
        }
    });
    use_context_provider({
        let hub = hub.clone();
        move || hub
    });

    // The daemon and the work host started, which lists the sessions from
    // then on; the user's key bindings read again whenever their file
    // changes, the host told when the user last typed into each session, and
    // the view state which sessions are live here, as their panes exit.
    use_hook({
        let panes = hub.panes.clone();
        let daemon = hub.daemon.clone();
        let hub = hub.clone();
        move || {
            spawn(async move {
                let compiled = tokio::task::spawn_blocking(|| {
                    daemon::install_terminfo(&daemon::terminfo_dir())
                })
                .await;
                if let Ok(Err(e)) = compiled {
                    app.status(Some(format!(
                        "cannot install the terminal's description: {e}"
                    )));
                }
                // The daemon may not run yet: started as adroit's own.
                if let Err(e) = daemon.start(&adroit_program(), &daemon::serve_args()).await {
                    app.status(Some(format!("cannot start the sessions' daemon: {e}")));
                }
                let standing = standing::settled(&daemon).await;
                app.dispatch(Msg::DaemonStanding(old_daemon_note(&standing)));
                // Replaced silently when of another build: it ends no session.
                if let Err(e) = work_host::ensure(
                    &work_host::socket(),
                    &adroit_program(),
                    &work_host::serve_args(),
                )
                .await
                {
                    app.status(Some(format!("cannot start the work host: {e}")));
                }
                app.watch();
                let mut keys_read = None::<Option<String>>;
                // What the keys file last failed with, to take back once it reads.
                let mut keys_err = None;
                let mut typed = BTreeMap::new();
                let mut told_live = None;
                loop {
                    // The user's key bindings, read again whenever the file changes.
                    let keys_text = std::fs::read_to_string(keys_path()).ok();
                    if keys_read.as_ref() != Some(&keys_text) {
                        let read = match keys_text.as_deref() {
                            Some(text) => Keymap::with_user(text),
                            None => Ok(Keymap::defaults()),
                        };
                        let failed = match read {
                            Ok(map) => {
                                app.dispatch(Msg::KeysRead(Box::new(map)));
                                None
                            }
                            Err(e) => Some(format!("{}: {e}", short(&keys_path()))),
                        };
                        app.while_failing(&mut keys_err, failed);
                        keys_read = Some(keys_text);
                    }
                    let inputs: BTreeMap<u128, u64> = panes
                        .borrow()
                        .iter()
                        .filter(|(tab, _)| matches!(tab, Tab::Session(_)))
                        .filter_map(|(_, pane)| pane.last_input_ms().map(|at| (pane.id(), at)))
                        .collect();
                    if inputs != typed {
                        typed = inputs.clone();
                        app.dispatch(Msg::Typed(inputs));
                    }
                    let live = live_sessions(&hub);
                    if told_live.as_ref() != Some(&live) {
                        told_live = Some(live.clone());
                        app.dispatch(Msg::PanesLive(live));
                    }
                    tokio::time::sleep(Duration::from_secs(2)).await;
                }
            });
        }
    });

    // What `@` in the prompt offers: the sessions, and the open worktree's
    // files, listed when the worktree changes. The memo passes a list on only
    // when it differs, so a session changing state resends nothing.
    let mut files = use_signal(Vec::<String>::new);
    use_effect(move || {
        let p = place();
        spawn(async move {
            let listed = match &p {
                Some(p) => Repo::open(&p.worktree).files().await.unwrap_or_default(),
                None => Vec::new(),
            };
            // A listing that took longer than the next one is for a worktree
            // no longer on screen.
            if app.place() == p {
                files.set(listed);
            }
        });
    });
    let offered = use_memo(move || candidates(&tree.read(), place.read().as_ref(), &files.read()));
    use_effect(move || {
        let Some(nv) = nvim() else { return };
        let list = offered();
        spawn(async move {
            let _ = nv.set_candidates(&list).await;
        });
    });
    // A path after `@` is walked from the open worktree.
    use_effect(move || {
        let Some(nv) = nvim() else { return };
        let root = place.read().as_ref().map(|p| p.worktree.clone());
        spawn(async move {
            let _ = nv.set_root(root.as_deref()).await;
        });
    });

    // What `/` offers: the agent's commands where the shown tab's session
    // runs, asked for when such a tab is shown or a `/` is typed and the list
    // there is missing or old; the old list is offered meanwhile.
    let mut command_cache = use_signal(HashMap::<adroit_model::Agent, commands::Cache>::new);
    let commands_here = use_memo(move || {
        let t = tree.read();
        let tab = active.read();
        let agent = draft_agent::for_tab(
            &t,
            tab.as_ref(),
            &app.content.read().drafts.options,
            app.content
                .read()
                .drafts
                .last_agent
                .unwrap_or(font.read().agent),
        );
        agents::adapter(agent)
            .capabilities()
            .command_probe
            .then(|| complete::commands_dir(&t, tab.as_ref(), place.read().as_ref()))
            .flatten()
            .map(|dir| (agent, dir))
    });
    let mut revalidate = move |(agent, dir): (adroit_model::Agent, PathBuf)| {
        if !command_cache
            .write()
            .entry(agent)
            .or_default()
            .want(&dir, Instant::now())
        {
            return;
        }
        spawn(async move {
            let got = agents::adapter(agent)
                .commands(crate::programs::agents(), dir.clone())
                .await;
            command_cache
                .write()
                .entry(agent)
                .or_default()
                .settle(&dir, Instant::now(), got);
        });
    };
    use_effect(move || {
        if let Some(dir) = commands_here() {
            revalidate(dir);
        }
    });
    let offered_commands = use_memo(move || match commands_here() {
        Some((agent, dir)) => {
            let mut list = agents::adapter(agent).builtin_commands();
            if let Some(cache) = command_cache.read().get(&agent) {
                list.extend_from_slice(cache.get(&dir));
            }
            complete::commands(&list)
        }
        None => Vec::new(),
    });
    use_effect(move || {
        let Some(nv) = nvim() else { return };
        let list = offered_commands();
        spawn(async move {
            let _ = nv.set_commands(&list).await;
        });
    });

    // What else the details panel reads by, as the window shows it.
    use_effect(move || {
        let t = tree.read();
        let tab = active.read().clone();
        let settings = font.read();
        let agent = draft_agent::for_tab(
            &t,
            tab.as_ref(),
            &app.content.read().drafts.options,
            app.content
                .read()
                .drafts
                .last_agent
                .unwrap_or(settings.agent),
        );
        let drawn = match &tab {
            Some(Tab::Session(id)) if nav::find(&t, id).is_some() => {
                pictures::drawn(&gallery.read().store, id)
            }
            _ => Vec::new(),
        };
        app.dispatch(Msg::DetailsShown(Box::new(details::Told {
            open: layout.read().show_details,
            usage: shows_usage(tab.as_ref(), agent).then_some(agent),
            drawn,
            settings: settings.details.clone(),
        })));
    });

    // The editor handoff belongs to the window, unlike terminal resources.
    use_hook({
        move || {
            let socket = socket_path(&instance_state());
            spawn(async move {
                let mut asked = match bind(&socket) {
                    Ok(listener) => {
                        // Only the window that answers on the socket can take
                        // Claude Code's input; another one would hand it to
                        // this one, which never asked.
                        match editor::install(&adroit_program(), &socket) {
                            Ok(_) => {
                                ANSWERS.store(true, Ordering::Relaxed);
                                app.dispatch(Msg::EditorAnswers);
                            }
                            Err(e) => {
                                app.status(Some(format!("cannot write the editor script: {e}")))
                            }
                        }
                        serve(listener)
                    }
                    Err(e) => {
                        // Opened together with another window of this
                        // instance, which claimed the socket first.
                        if matches!(ask(&socket, &Request::Show), Ok(Response::Shown)) {
                            dioxus::desktop::window().close();
                            return;
                        }
                        app.status(Some(format!("cannot answer window requests: {e}")));
                        return;
                    }
                };
                while let Some((request, reply)) = asked.recv().await {
                    if let Request::Editor { path } = &request {
                        let _ = reply.send(take_input(path, prompts, app).await);
                        continue;
                    }
                    if request == Request::Show {
                        let w = dioxus::desktop::window();
                        w.window.set_visible(true);
                        w.window.set_minimized(false);
                        w.window.set_focus();
                        let _ = reply.send(Response::Shown);
                        continue;
                    }
                }
            });
        }
    });

    // The cell the console's font draws: as wide as its characters and as
    // tall as the font's own lines, as a terminal sizes it, measured again
    // whenever the font changes or the window is zoomed, with the room the
    // console has.
    use_effect(move || {
        let _ = font();
        let _ = zoomed();
        spawn(async move {
            // A zoom is laid out after the frame it was asked in.
            tokio::time::sleep(Duration::from_millis(50)).await;
            let mut e = document::eval(MEASURE);
            if let Ok((w, h)) = e.recv::<(f64, f64)>().await
                && w > 0.0
                && h > 0.0
            {
                let h = h.round();
                cell_height_now.set(h);
                cell_width.set(w);
            }
            // A resize is not told of a zoom, which changes the room in the
            // console's own pixels all the same.
            let mut room = document::eval(
                "const c = document.querySelector('.win.focused > .console, .content-windows > .console');
                 const r = c ? c.getBoundingClientRect() : {width: 0, height: 0};
                 dioxus.send([r.width, r.height]);",
            );
            if let Ok((w, h)) = room.recv::<(f64, f64)>().await
                && w > 0.0
                && *console_box.peek() != (w, h)
            {
                console_box.set((w, h));
            }
        });
    });
    // The prompt editor, started again when it quits, as `:q` makes it.
    use_hook(move || {
        spawn(async move {
            loop {
                let (tx, mut rx) = mpsc::unbounded_channel();
                let started = Embedded::spawn(&nvim_program(), &[], 80, 1, move |ev| {
                    let _ = tx.send(ev);
                })
                .await;
                let embedded = match started {
                    Ok(e) => Arc::new(e),
                    Err(e) => {
                        app.status(Some(format!("cannot start Neovim: {e}")));
                        return;
                    }
                };
                let born = Instant::now();
                nvim.set(Some(embedded.clone()));
                app.dispatch(Msg::PromptRuns(true));
                let rows = *prompt_rows.peek() as u16;
                let cols = *prompt_cols.peek();
                let _ = embedded.resize(cols, rows).await;
                while let Some(ev) = rx.recv().await {
                    match ev {
                        NvimEvent::Redraw => {
                            *prompt_frame.write() += 1;
                            let m = mode_of(&embedded.state());
                            if *prompt_mode.peek() != m {
                                prompt_mode.set(m);
                            }
                        }
                        // Only the console's reader tells of yanks and of
                        // where it is scrolled to.
                        NvimEvent::Yank { .. } | NvimEvent::Scrolled { .. } => {}
                        NvimEvent::Shell(on) => prompt_shell.set(on),
                        NvimEvent::CommandsWanted => {
                            if let Some(dir) = commands_here.peek().clone() {
                                revalidate(dir);
                            }
                        }
                        NvimEvent::View { top, lines } => {
                            if *prompt_view.peek() != (top, lines) {
                                prompt_view.set((top, lines));
                            }
                        }
                        NvimEvent::Height(h) => {
                            prompt_height.set(h);
                            let s = settings::current();
                            let rows = crate::console::prompt_rows(
                                h,
                                !shown_images.peek().is_empty(),
                                s.prompt_min_rows,
                                s.prompt_max_rows,
                            );
                            if rows != *prompt_rows.peek() {
                                prompt_rows.set(rows);
                                let cols = *prompt_cols.peek();
                                let _ = embedded.resize(cols, rows as u16).await;
                            }
                        }
                    }
                }
                nvim.set(None);
                app.dispatch(Msg::PromptRuns(false));
                // One that quits as it starts would be started forever.
                if born.elapsed() < NVIM_SETTLE {
                    app.status(Some(
                        "Neovim quit as it started; restart adroit to try again".into(),
                    ));
                    return;
                }
                app.notice("Neovim quit; the prompt starts afresh".into());
            }
        });
    });

    // The console's terminal fills what the prompt card leaves of the console,
    // in cells of the measured width; all of it when no card is shown.
    use_effect(move || {
        let (width, height) = console_box();
        let cw = cell_width();
        let _ = (font(), cell_height_now());
        if width <= 0.0 {
            return;
        }
        let card = shows_prompt(active().as_ref()).then_some(composer_height());
        let free = console_room(height, card);
        let next = Size {
            cell_width: cw,
            cell_height: cell_height_now(),
            ..Size::new(
                ((width - 24.0) / cw).floor().max(10.0) as u16,
                (free / cell_height_now()).floor().max(4.0) as u16,
            )
        };
        if *console_size.peek() != next {
            console_size.set(next);
        }
    });

    // The prompt's grid is as wide as its box allows.
    use_effect(move || {
        let width = prompt_width();
        let cw = cell_width();
        let nv = nvim();
        if width <= 0.0 {
            return;
        }
        let cols = ((width - 28.0) / cw).floor().max(10.0) as u16;
        if cols != *prompt_cols.peek() {
            prompt_cols.set(cols);
        }
        if let Some(nv) = nv {
            let rows = *prompt_rows.peek() as u16;
            spawn(async move {
                let _ = nv.resize(cols, rows).await;
            });
        }
    });

    // Every other window of the shown tab shows its program at its own size;
    // one the focused window shows as well keeps the focused window's size.
    use_effect({
        let hub = hub.clone();
        move || {
            // Taken out of the view state first: attaching tells it which
            // sessions are live.
            let windows: Vec<(WindowId, Tab)> = {
                let view = app.content.read();
                let Some(tab) = view.windows.current() else {
                    return;
                };
                let focused = view.active();
                let mut sized = HashSet::new();
                tab.windows()
                    .into_iter()
                    .filter(|w| {
                        Some(&w.shows) != focused
                            && !matches!(w.shows, Tab::Draft { .. })
                            && sized.insert(w.shows.clone())
                    })
                    .map(|w| (w.id, w.shows.clone()))
                    .collect()
            };
            let boxes = win_boxes.read().clone();
            let cw = cell_width();
            let ch = cell_height_now();
            let _ = (font(), tree.read(), attached());
            for (id, shows) in windows {
                let Some(&(width, height)) = boxes.get(&id).filter(|(w, _)| *w > 0.0) else {
                    continue;
                };
                let size = Size {
                    cell_width: cw,
                    cell_height: ch,
                    ..Size::new(
                        ((width - 24.0) / cw).floor().max(10.0) as u16,
                        (console_room(height, None) / ch).floor().max(4.0) as u16,
                    )
                };
                let me = shows.clone();
                let pane = ensure_pane(app, &hub, &shows, size, move || {
                    *attached.write() += 1;
                    app.dispatch(Msg::InputMayHold(me));
                });
                if let Some(pane) = pane {
                    pane.resize(size);
                }
            }
        }
    });

    // Attach the active tab's session at the console's size.
    use_effect({
        let hub = hub.clone();
        move || {
            let Some(tab) = active() else { return };
            let size = console_size();
            // Read, so a session listed only now is attached to once it is,
            // and one attached is sized to the console as it is by then.
            let _ = (tree.read(), attached());
            let me = tab.clone();
            let pane = ensure_pane(app, &hub, &tab, size, move || {
                *attached.write() += 1;
                app.dispatch(Msg::InputMayHold(me));
            });
            if let Some(pane) = pane {
                pane.resize(size);
            }
        }
    });

    // Keep what is selected in the sidebar in view, after it is drawn.
    let selected = use_memo(move || app.tree.read().selected.clone());
    use_effect(move || {
        let _ = selected();
        document::eval(
            "requestAnimationFrame(() => document.querySelectorAll('.sidebar .list-row.selected')
               .forEach((r) => r.scrollIntoView({ block: 'nearest' })));",
        );
    });

    // And the tab the console shows, along the one line of tabs.
    use_effect(move || {
        let _ = active();
        document::eval(
            "requestAnimationFrame(() => document.querySelectorAll('.tabs .tab.active')
               .forEach((t) => t.scrollIntoView({ block: 'nearest', inline: 'nearest' })));",
        );
    });

    // The filter's field, which takes the keys while it is typed into.
    let filter_field = use_signal(|| None::<Rc<MountedData>>);
    let focus_filter = move |on: bool| {
        if let Some(field) = filter_field.peek().clone() {
            spawn(async move {
                let _ = field.set_focus(on).await;
            });
        }
    };
    // Leaving the sidebar, or opening a menu, stops typing into the filter,
    // so the keys a menu names run what they name.
    use_effect(move || {
        let left = focus() != Area::Sidebar || app.overlays.read().menu.is_some();
        if left && app.tree.peek().filtering {
            focus_filter(false);
        }
    });
    // Keep sessions visible while a prompt is in flight, even across tab switches.
    let mut sending = use_signal(|| false);

    // The prompt shows what the focused window shows.
    use_effect(move || {
        let now = active.read().clone();
        let _ = prompts.send(Op::Show(now));
    });

    // A shell or a Neovim asked for starts here, where the hub is.
    let start_terminal = {
        let hub = hub.clone();
        move |ask: TerminalAsk| {
            let TerminalAsk {
                place: p,
                runs,
                file,
                opening,
                focus: take_keys,
            } = ask;
            let names: Vec<String> = terminals
                .peek()
                .iter()
                .filter(|s| s.cwd == p.worktree)
                .map(|s| s.name.clone())
                .collect();
            let taken: Vec<&str> = names.iter().map(String::as_str).collect();
            let base = match (&file, runs) {
                (Some(f), _) => file_name(f),
                (None, runs) => crate::terminals::base_name(runs, &[]),
            };
            let name = fresh_name(&base, &taken);
            let size = *console_size.peek();
            let mut resolved = crate::execution::resolve_terminal(
                &settings::current(),
                crate::execution::TerminalStartRequest {
                    name,
                    cwd: p.worktree.clone(),
                    program: runs,
                    // Not `agent_env`: a shell or Neovim keeps the user's own editor.
                    env: daemon::terminal_env(),
                    file,
                    command: Vec::new(),
                },
                &programs::login_shell(),
            );
            resolved.spawn.cols = size.cols;
            resolved.spawn.rows = size.rows;
            let hub = hub.clone();
            spawn(async move {
                match start(app, &hub, resolved.spawn, size, Tab::Terminal, None).await {
                    Ok(id) => {
                        resolved.terminal.id = id;
                        app.dispatch(Msg::TerminalOpened(resolved.terminal));
                        app.dispatch(Msg::Show {
                            thing: Tab::Terminal(id),
                            place: Some(p),
                            how: Some(opening),
                        });
                        if take_keys {
                            app.dispatch(Msg::Focus(Area::Console));
                        }
                    }
                    Err(e) => app.status(Some(format!("cannot start {base}: {e}"))),
                }
            });
        }
    };
    // Open a row where `how` says, or as anything else opens a thing when it
    // says nothing; `take_keys` gives the keys to what opened.
    let open = move |item: Item, how: Option<Opening>, take_keys: bool| {
        // Typing stops going to the filter once a row is taken; what it
        // narrowed to stays.
        if app.tree.peek().filtering {
            focus_filter(false);
        }
        app.dispatch(Msg::OpenRow {
            item,
            how,
            take_keys,
        });
    };

    let submit = {
        let hub = hub.clone();
        move || {
            let hub = hub.clone();
            if *sending.peek() || !shows_prompt(app.active().as_ref()) {
                return;
            }
            sending.set(true);
            spawn(async move {
                // Taken now, so what is typed while it is sent stays for the
                // next prompt; put back if it cannot be sent. It goes to the
                // tab it was written for, though another be shown by now.
                let (give, given) = oneshot::channel();
                let taken = match prompts.send(Op::Take(give)) {
                    Ok(()) => given.await.ok(),
                    Err(_) => None,
                };
                // Sent empty, the key itself goes to the session, as it would in
                // Claude Code's own input.
                if let Some(id) = taken
                    .as_ref()
                    .filter(|(_, _, images)| images.is_empty())
                    .and_then(|(tab, text, _)| passes_enter(tab.as_ref(), text))
                {
                    let size = *console_size.peek();
                    if let Some(p) = pane_for(app, &hub, &Tab::Session(id), size).await {
                        p.key(&Key::new("Enter").ctrl());
                    }
                }
                let Some((shown, text, images)) =
                    taken.filter(|(_, text, images)| !text.trim().is_empty() || !images.is_empty())
                else {
                    sending.set(false);
                    return;
                };
                let nv = nvim.peek().clone();
                if let Some(nv) = nv {
                    let _ = nv.stop_writing().await;
                }
                // Sent with nothing open, a prompt starts a scratch session, as
                // a draft there would.
                let tab = match shown {
                    None => {
                        app.dispatch(Msg::Draft {
                            worktree: scratch_dir(),
                            write: false,
                        });
                        app.active().unwrap_or_else(|| Tab::draft(scratch_dir()))
                    }
                    Some(shown) => shown,
                };
                let sent_to = Some(tab.clone());
                let size = *console_size.peek();
                let t = tree.peek().clone();
                let launch_settings = settings::current();
                let agent = draft_agent::for_tab(
                    &t,
                    Some(&tab),
                    &app.content.peek().drafts.options,
                    app.content
                        .peek()
                        .drafts
                        .last_agent
                        .unwrap_or(launch_settings.agent),
                );
                let waiting =
                    |id: &str| nav::find(&t, id).is_some_and(|(_, s)| s.state.takes_every_key());
                let sent = match tab {
                    // The Enter that ends a prompt would pick the choice in
                    // front, usually Yes; the prompt waits for it to be answered.
                    Tab::Session(id) if waiting(&id) => {
                        // The keys go where the answer is given; what was written
                        // waits in the prompt for after.
                        app.dispatch(Msg::Focus(Area::Console));
                        app.notice(
                            "Answer the session above first; your message waits here".into(),
                        );
                        false
                    }
                    _ if !images.is_empty()
                        && agents::adapter(agent).capabilities().shell_bang
                        && adroit_agents::claude::shell_line(&text).is_some() =>
                    {
                        app.notice("Images cannot be sent with a shell command".into());
                        false
                    }
                    _ if images.iter().any(|path| !path.is_file()) => {
                        app.notice(
                            "An attached image is unavailable; remove it or paste it again".into(),
                        );
                        false
                    }
                    tab @ Tab::Session(_) => match pane_for(app, &hub, &tab, size).await {
                        Some(p) => {
                            // Claude Code turns on bracketed paste once it is
                            // ready for input.
                            for _ in 0..ATTACH_WAIT_STEPS {
                                if p.bracketed_paste() || p.exited() {
                                    break;
                                }
                                tokio::time::sleep(Duration::from_millis(50)).await;
                            }
                            if p.exited() {
                                app.status(Some(
                                    "The session ended before it took the prompt".into(),
                                ));
                                false
                            } else {
                                match adroit_agents::claude::shell_line(&text)
                                    .filter(|_| agents::adapter(agent).capabilities().shell_bang)
                                {
                                    Some(command) => {
                                        p.type_text("!");
                                        tokio::time::sleep(Duration::from_millis(40)).await;
                                        p.paste(command);
                                    }
                                    None => {
                                        let adapter = agents::adapter(agent);
                                        for image in adapter.image_pastes(&images) {
                                            p.paste(&image);
                                            tokio::time::sleep(Duration::from_millis(80)).await;
                                        }
                                        let message = adapter.image_prompt(&text, &images);
                                        if !message.is_empty() {
                                            p.paste(&message);
                                        }
                                    }
                                }
                                tokio::time::sleep(Duration::from_millis(40)).await;
                                p.key(&Key::new("Enter"));
                                true
                            }
                        }
                        None => {
                            app.status(Some("This session cannot be reached from here".into()));
                            false
                        }
                    },
                    Tab::Terminal(_) | Tab::Viewer(_) => false,
                    Tab::Draft { .. }
                        if named(&text).0.is_some()
                            && named(&text).1.trim().is_empty()
                            && images.is_empty() =>
                    {
                        app.notice("Write what the session is to do below its name".into());
                        false
                    }
                    // Given as the first prompt, a shell line is not known to
                    // run as a command; it could reach the model as a message.
                    Tab::Draft { .. }
                        if agents::adapter(agent).capabilities().shell_bang
                            && adroit_agents::claude::shell_line(&text).is_some() =>
                    {
                        app.notice(
                            "A shell line runs in a session: start it with a message first".into(),
                        );
                        false
                    }
                    Tab::Draft { worktree, id } => {
                        let (given, text) = named(&text);
                        let label = given.unwrap_or_else(|| label_from(&text));
                        let taken: Vec<&str> = t.sessions().map(|(_, s)| s.name.as_str()).collect();
                        let name = fresh_name(&label, &taken);
                        let options = app
                            .content
                            .peek()
                            .drafts
                            .options
                            .get(&Tab::Draft {
                                worktree: worktree.clone(),
                                id,
                            })
                            .cloned()
                            .unwrap_or_default();
                        let draft = Tab::Draft {
                            worktree: worktree.clone(),
                            id,
                        };
                        // Made only now, so a draft left unsent leaves no
                        // empty directory behind.
                        let started =
                            match scratch::start_dir(&worktree, &scratch_dir(), Moment::now()) {
                                Ok(worktree) => {
                                    let request = crate::execution::StartRequest {
                                        agent,
                                        name: name.clone(),
                                        cwd: worktree,
                                        model: options.model,
                                        permissions: options.permissions,
                                        prompt: text,
                                        images: images.clone(),
                                        env: daemon::agent_env(),
                                        started_ms: now_ms(),
                                        ..Default::default()
                                    };
                                    match crate::delegation::app::prepare(
                                        &crate::delegation::app::root(),
                                        &launch_settings,
                                        request,
                                        None,
                                        &adroit_program(),
                                    )
                                    .await
                                    {
                                        Ok((resolved, recorded)) => {
                                            let mut spawn = resolved.spawn;
                                            spawn.cols = size.cols;
                                            spawn.rows = size.rows;
                                            start(
                                                app,
                                                &hub,
                                                spawn,
                                                size,
                                                |id| Tab::Session(sessions::hex(id)),
                                                Some(recorded),
                                            )
                                            .await
                                            .map_err(|e| format!("cannot start {name}: {e}"))
                                        }
                                        Err(e) => Err(e),
                                    }
                                }
                                Err(e) => Err(format!("cannot make a directory for {name}: {e}")),
                            };
                        match started {
                            Ok(id) => {
                                let id = sessions::hex(id);
                                app.dispatch(Msg::Relist);
                                app.dispatch(Msg::TreeSelect(Some(Item::Session(id.clone()))));
                                app.dispatch(Msg::Replace {
                                    from: draft,
                                    to: Tab::Session(id),
                                });
                                true
                            }
                            Err(e) => {
                                app.status(Some(e));
                                false
                            }
                        }
                    }
                };
                if sent {
                    let _ = prompts.send(Op::Sent);
                } else {
                    let lines: Vec<String> = text.lines().map(String::from).collect();
                    let _ = prompts.send(Op::RestoreImages(sent_to.clone(), images));
                    let _ = prompts.send(Op::Add(sent_to, lines, None));
                }
                sending.set(false);
            });
        }
    };

    let choose_agent = move |tab: Option<Tab>, agent| app.dispatch(Msg::DraftAgent { tab, agent });

    // A session started already keeps its own agent.
    let switch_agent = move |forward: bool| match app.active() {
        tab if draft_agent::can_choose(tab.as_ref()) => {
            let now = draft_agent::for_tab(
                &tree.peek(),
                tab.as_ref(),
                &app.content.peek().drafts.options,
                app.content
                    .peek()
                    .drafts
                    .last_agent
                    .unwrap_or(settings::current().agent),
            );
            choose_agent(tab, if forward { now.next() } else { now.prev() });
        }
        _ => app.notice("Only a session not started yet can change its agent".into()),
    };

    // Continue a conversation an agent kept in a new session, in the
    // directory it ran in, shown where the user is.
    let resume = {
        let hub = hub.clone();
        move |past: adroit_model::Past| {
            let hub = hub.clone();
            spawn(async move {
                let size = *console_size.peek();
                let taken: Vec<String> = tree
                    .peek()
                    .sessions()
                    .map(|(_, s)| s.name.clone())
                    .collect();
                let taken: Vec<&str> = taken.iter().map(String::as_str).collect();
                let name = fresh_name(&label_from(&past.title), &taken);
                app.status(Some(format!("resuming {name}…")));
                let request = crate::execution::StartRequest {
                    agent: past.agent,
                    name: name.clone(),
                    cwd: past.cwd.clone(),
                    resume: Some(past.id.clone()),
                    env: daemon::agent_env(),
                    started_ms: now_ms(),
                    ..Default::default()
                };
                let started = match crate::delegation::app::prepare(
                    &crate::delegation::app::root(),
                    &settings::current(),
                    request,
                    None,
                    &adroit_program(),
                )
                .await
                {
                    Ok((resolved, recorded)) => {
                        let mut spawn = resolved.spawn;
                        spawn.cols = size.cols;
                        spawn.rows = size.rows;
                        let tab_of = |id| Tab::Session(sessions::hex(id));
                        start(app, &hub, spawn, size, tab_of, Some(recorded))
                            .await
                            .map_err(|e| e.to_string())
                    }
                    Err(e) => Err(e),
                };
                match started {
                    Ok(id) => {
                        app.status(None);
                        let id = sessions::hex(id);
                        app.dispatch(Msg::Relist);
                        app.dispatch(Msg::TreeSelect(Some(Item::Session(id.clone()))));
                        app.dispatch(Msg::Show {
                            thing: Tab::Session(id),
                            place: None,
                            how: None,
                        });
                    }
                    Err(e) => app.status(Some(format!("cannot resume {name}: {e}"))),
                }
            });
        }
    };

    // The settings the Settings window changes are applied as they change.
    use_hook({
        let hub = hub.clone();
        move || {
            spawn(async move {
                let mut changed = settings::CHANGED.subscribe();
                let mut was = settings::current();
                while changed.recv().await.is_ok() {
                    let s = settings::current();
                    app.dispatch(Msg::ZoomSetting(s.zoom));
                    let face = |s: &settings::Settings| {
                        (s.font_family.clone(), s.font_size, s.font_weight)
                    };
                    if face(&s) != face(&was) || s.agent != was.agent || s.usage != was.usage {
                        font.set(s.clone());
                    }
                    // A new theme reaches every console open, what is drawn and
                    // what a program asking for its colours is told.
                    if s.theme != was.theme {
                        match themes::palette(&s.theme, &crate::settings::themes_dir()) {
                            Ok(p) => {
                                for pane in hub.panes.borrow().values() {
                                    pane.set_palette(&p);
                                }
                                let mut palette = hub.palette;
                                palette.set(p);
                            }
                            Err(e) => app.status(Some(e)),
                        }
                    }
                    let rows = crate::console::prompt_rows(
                        *prompt_height.peek(),
                        !shown_images.peek().is_empty(),
                        s.prompt_min_rows,
                        s.prompt_max_rows,
                    );
                    if rows != *prompt_rows.peek() {
                        prompt_rows.set(rows);
                        let embedded = nvim.peek().clone();
                        if let Some(e) = embedded {
                            let cols = *prompt_cols.peek();
                            let _ = e.resize(cols, rows as u16).await;
                        }
                    }
                    was = s;
                }
            });
        }
    });
    use_drop(settings_window::close);
    use_drop(image_window::close_all);

    // End a session adroit runs; Claude Code keeps its conversation.
    let stop = {
        let hub = hub.clone();
        move |asked: Session| {
            let hub = hub.clone();
            spawn(async move {
                let Some(id) = sessions::id_of(&asked.id) else {
                    return;
                };
                match hub.daemon.end(id).await {
                    Ok(()) => {
                        app.notice(format!("ended {}", asked.name));
                        if let Some(record) = asked.delegation {
                            let root = crate::delegation::app::root();
                            if let Err(error) = crate::delegation::app::ended(&root, record).await {
                                app.status(Some(format!(
                                    "ended {}, but cannot record its exit: {error}",
                                    asked.name
                                )));
                            }
                        }
                    }
                    Err(e) => app.status(Some(format!("cannot end {}: {e}", asked.name))),
                }
                app.dispatch(Msg::Relist);
            });
        }
    };

    let kill_process = move |session: Session, process: crate::processes::Process| {
        spawn(async move {
            let pid = process.pid;
            let result =
                tokio::task::spawn_blocking(move || crate::processes::kill(&session, &process))
                    .await;
            match result {
                Ok(Ok(())) => app.notice(format!("Killed process {pid}")),
                Ok(Err(e)) => app.notice(e),
                Err(e) => app.notice(format!("Cannot kill process: {e}")),
            }
            app.dispatch(Msg::DetailsReadAgain);
        });
    };

    // Replace the daemon with one of this build, ending every session it holds.
    let restart_daemon = {
        let hub = hub.clone();
        move || {
            let hub = hub.clone();
            spawn(async move {
                app.notice("Restarting the sessions' daemon".into());
                match hub
                    .daemon
                    .restart(&adroit_program(), &daemon::serve_args())
                    .await
                {
                    Ok(()) => app.notice("Restarted the sessions' daemon".into()),
                    Err(e) => app.status(Some(format!("cannot restart the sessions' daemon: {e}"))),
                }
                let standing = standing::settled(&hub.daemon).await;
                app.dispatch(Msg::DaemonRestarted(old_daemon_note(&standing)));
                app.dispatch(Msg::Relist);
            });
        }
    };

    // Name a session anew: in its tags, which the sidebar shows, and to
    // Claude Code with its `/rename`, so its own interface says the same.
    let send_rename = {
        let hub = hub.clone();
        move |r: Rename| {
            let (Some(name), Some(id)) = (r.name(), sessions::id_of(&r.id)) else {
                return;
            };
            let agent = draft_agent::for_tab(
                &tree.peek(),
                Some(&Tab::Session(r.id.clone())),
                &HashMap::new(),
                settings::current().agent,
            );
            let hub = hub.clone();
            spawn(async move {
                let held = hub
                    .daemon
                    .sessions()
                    .await
                    .ok()
                    .and_then(|all| all.into_iter().find(|s| s.id == id))
                    .map(|s| adroit_term::tags::holding(&s.tags, sessions::NAME))
                    .unwrap_or_default();
                let named = adroit_term::tags::tagged(sessions::NAME, &name);
                if let Err(e) = hub.daemon.tag(id, named, held).await {
                    app.status(Some(format!("cannot rename {}: {e}", r.was)));
                    return;
                }
                app.dispatch(Msg::Relist);
                let tab = Tab::Session(r.id.clone());
                let size = *console_size.peek();
                if let Some(command) = agents::adapter(agent).rename_command(&name)
                    && let Some(p) = pane_for(app, &hub, &tab, size).await
                {
                    p.paste(&command);
                    tokio::time::sleep(Duration::from_millis(40)).await;
                    p.key(&Key::new("Enter"));
                }
            });
        }
    };

    let active_pane = {
        let hub = hub.clone();
        move || -> Option<Arc<Pane>> {
            let tab = app.active()?;
            hub.panes.borrow().get(&tab).cloned()
        }
    };

    // Told when the console changes while it is read.
    let console_changed = use_hook(|| Rc::new(Notify::new()));
    use_effect({
        let console_changed = console_changed.clone();
        move || {
            let _ = frame();
            if app.console.read().reading {
                console_changed.notify_one();
            }
        }
    });

    // The console's reader: started once, sized as the console, shown what
    // the console holds again as it changes while it is read.
    use_hook({
        let active_pane = active_pane.clone();
        let hub = hub.clone();
        let console_changed = console_changed.clone();
        move || {
            spawn(async move {
                let (tx, mut rx) = mpsc::unbounded_channel();
                let size = *console_size.peek();
                let started =
                    Embedded::spawn_reader(&nvim_program(), &[], size.cols, size.rows, move |ev| {
                        let _ = tx.send(ev);
                    })
                    .await;
                let Ok(r) = started else { return };
                reader.set(Some(Arc::new(r)));
                while let Some(ev) = rx.recv().await {
                    match ev {
                        NvimEvent::Redraw => *reader_frame.write() += 1,
                        NvimEvent::Scrolled { top, left } => reader_view.set((top, left)),
                        NvimEvent::Yank { text, lines } => match copy_to_clipboard(&text) {
                            Ok(()) if lines == 1 => app.notice("Copied a line".into()),
                            Ok(()) => app.notice(format!("Copied {lines} lines")),
                            Err(e) => app.notice(format!("cannot copy: {e}")),
                        },
                        _ => {}
                    }
                }
            });
            spawn(async move {
                let mut shown = None::<u64>;
                loop {
                    console_changed.notified().await;
                    if !app.console.peek().reading {
                        shown = None;
                        continue;
                    }
                    let f = *frame.peek();
                    if shown == Some(f) {
                        continue;
                    }
                    shown = Some(f);
                    let (Some(p), Some(r)) = (active_pane(), reader.peek().clone()) else {
                        continue;
                    };
                    let palette = hub.palette.peek().clone();
                    let cols = console_size.peek().cols;
                    let Ok(mut all) = p.everything(&hub.daemon, cols).await else {
                        continue;
                    };
                    // The cursor is where the screen, at the end, has it.
                    let screen = p.screen();
                    let above = all.rows.saturating_sub(screen.rows);
                    all.cursor = screen.cursor.map(|c| adroit_term::Cursor {
                        row: c.row + above,
                        ..c
                    });
                    read_rows.set((all.rows, screen.rows));
                    let (lines, marks, cur) = console_read::reading(&all, palette.foreground);
                    let _ = r
                        .show(&lines, &marks, cur, false, &all.background.hex())
                        .await;
                    tokio::time::sleep(READER_GAP).await;
                }
            });
        }
    });
    use_effect(move || {
        let size = console_size();
        if let Some(r) = reader.peek().clone() {
            spawn(async move {
                let _ = r.resize(size.cols, size.rows).await;
            });
        }
    });
    // A session left scrolled back would show its jump-to-bottom line live,
    // moving with its layout.
    let console_live = {
        let active_pane = active_pane.clone();
        move || {
            if let Some(p) = active_pane().filter(|p| p.takes_mouse()) {
                p.key(&reader::newest());
            }
        }
    };
    // Start reading the console, the cursor where the console's is, and give
    // the reader `keys`, the ones that started it.
    let enter_reading = {
        let active_pane = active_pane.clone();
        let hub = hub.clone();
        move |keys: Vec<String>| {
            let (Some(p), Some(r)) = (active_pane(), reader.peek().clone()) else {
                return;
            };
            let hub = hub.clone();
            spawn(async move {
                let palette = hub.palette.peek().clone();
                let cols = console_size.peek().cols;
                let Ok(mut all) = p.everything(&hub.daemon, cols).await else {
                    return;
                };
                let screen = p.screen();
                let above = all.rows.saturating_sub(screen.rows);
                all.cursor = screen.cursor.map(|c| adroit_term::Cursor {
                    row: c.row + above,
                    ..c
                });
                read_rows.set((all.rows, screen.rows));
                let (lines, marks, cur) = console_read::reading(&all, palette.foreground);
                let _ = r.input("<C-\\><C-N>").await;
                if r.show(&lines, &marks, cur, true, &all.background.hex())
                    .await
                    .is_ok()
                {
                    app.dispatch(Msg::ReadingStarted);
                    let _ = r.input(&keys.concat()).await;
                }
            });
        }
    };

    // The wheel over the console's reader: it scrolls the reader, and past
    // the first or last line it holds, a program that scrolls what it shows
    // itself, which the reader then follows.
    let scroll_reader = {
        let active_pane = active_pane.clone();
        move |lines: i32| {
            let Some(r) = reader.peek().clone() else {
                return;
            };
            let top = reader_view.peek().0;
            if let Some(p) = active_pane().filter(|p| p.takes_mouse()) {
                let page = r.state().screen().rows;
                let held = read_rows.peek().0;
                let past = (lines > 0 && top == 0) || (lines < 0 && top + page >= held);
                if past {
                    p.wheel(lines, 0, 0);
                    *frame.write() += 1;
                    return;
                }
            }
            let keys = if lines > 0 {
                format!("{lines}<C-y>")
            } else {
                format!("{}<C-e>", -lines)
            };
            spawn(async move {
                let _ = r.input(&keys).await;
            });
        }
    };

    // A click on the reader goes to the cell of the program's screen under
    // it; the screen is the last of the lines the reader holds.
    let click_reader = {
        let active_pane = active_pane.clone();
        move |(col, row): (usize, usize)| {
            let Some(p) = active_pane() else {
                return;
            };
            let (top, left) = *reader_view.peek();
            let (held, screen) = *read_rows.peek();
            if let Some(row) = (top + row).checked_sub(held.saturating_sub(screen)) {
                p.click(left + col, row);
            }
        }
    };

    // Claude Code reports no status for a dialog it opens itself, so the
    // console's screen tells, looked at again whenever it is redrawn.
    use_effect({
        let active_pane = active_pane.clone();
        move || {
            let _ = frame();
            let shows = match &*active.read() {
                Some(tab @ Tab::Session(_)) => {
                    let agent = draft_agent::for_tab(
                        &tree.read(),
                        Some(tab),
                        &app.content.read().drafts.options,
                        app.content
                            .read()
                            .drafts
                            .last_agent
                            .unwrap_or(font.read().agent),
                    );
                    active_pane()
                        .is_some_and(|p| agents::adapter(agent).dialog_open(&p.screen().text()))
                }
                _ => false,
            };
            if app.console.peek().dialog != shows {
                app.dispatch(Msg::Dialog(shows));
            }
        }
    });

    // Ask the session in the console to hand its input over: Ctrl-g opens
    // its external editor, which is `adroit editor`.
    let ask_input = {
        let hub = hub.clone();
        move |tab: Tab| {
            let pane = hub.panes.borrow().get(&tab).cloned();
            if let Some(p) = pane.filter(|p| !p.exited()) {
                p.type_text("\x07");
                app.dispatch(Msg::InputAsked(tab));
            }
        }
    };
    // What the view state asks of the panes, the prompt and the console,
    // which live here.
    use_hook({
        let panes = hub.panes.clone();
        let ask_input = ask_input.clone();
        let console_live = console_live.clone();
        let enter_reading = enter_reading.clone();
        let active_pane = active_pane.clone();
        let start_terminal = start_terminal.clone();
        let resume = resume.clone();
        let stop = stop.clone();
        let restart_daemon = restart_daemon.clone();
        let send_rename = send_rename.clone();
        move || {
            app.hand_over(move |effect| match effect {
                CoreEffect::Resume(past) => resume(past),
                CoreEffect::Stop(session) => stop(session),
                CoreEffect::KillProcess { session, process } => kill_process(session, process),
                CoreEffect::RestartDaemon => restart_daemon(),
                CoreEffect::Rename(r) => send_rename(r),
                CoreEffect::ShowShot { shot, session } => {
                    show_shot(&shot, &session, &gallery.peek())
                }
                CoreEffect::StartTerminal(ask) => start_terminal(ask),
                CoreEffect::PromptClose(to) => {
                    let _ = prompts.send(Op::Close(to));
                }
                CoreEffect::StartWriting => start_writing(nvim),
                CoreEffect::PaneKey(key) => {
                    if let Some(p) = active_pane() {
                        p.key(&key);
                    }
                }
                CoreEffect::PaneScrollKey(key) => {
                    if let Some(p) = active_pane() {
                        p.key(&key);
                        let mut frame = frame;
                        *frame.write() += 1;
                    }
                }
                CoreEffect::PaneText { text, pasted } => {
                    if let Some(p) = active_pane() {
                        if pasted {
                            p.paste(&text);
                        } else {
                            p.type_text(&text);
                        }
                    }
                }
                CoreEffect::PaneWheel(lines) => {
                    if let Some(p) = active_pane() {
                        p.wheel(lines, 0, 0);
                        let mut frame = frame;
                        *frame.write() += 1;
                    }
                }
                CoreEffect::ReaderKey(key) => {
                    if let (Some(r), Some(n)) = (reader.peek().clone(), notation(&key)) {
                        spawn(async move {
                            let _ = r.input(&n).await;
                        });
                    }
                }
                CoreEffect::ReadConsole(keys) => {
                    let enter = enter_reading.clone();
                    enter(keys.iter().filter_map(notation).collect());
                }
                CoreEffect::ConsoleLive => console_live(),
                CoreEffect::PromptKey(key) => {
                    if let Some(n) = notation(&key)
                        && let Some(nv) = nvim.peek().clone()
                    {
                        spawn(async move {
                            if let Err(e) = nv.input(&n).await {
                                keys::trace(|| format!("prompt refused {n}: {e}"));
                            }
                        });
                    }
                }
                CoreEffect::PromptText { text, pasted } => {
                    if let Some(nv) = nvim.peek().clone() {
                        spawn(async move {
                            if pasted {
                                let _ = nv.paste(&text).await;
                            } else {
                                let _ = nv.type_text(&text).await;
                            }
                        });
                    }
                }
                CoreEffect::KeepImage { mime, data } => {
                    let dir = pasted::dir();
                    let now = std::time::SystemTime::now();
                    pasted::prune(&dir, now);
                    match pasted::save(&dir, &mime, &data, now) {
                        Ok(path) => {
                            let _ = prompts.send(Op::Attach(path));
                        }
                        Err(e) => app.notice(e.to_string()),
                    }
                }
                CoreEffect::AskInput(tab) => ask_input(tab),
                // A session no longer listed has nothing to show; its pane is
                // closed, and the images its console drew are not kept.
                CoreEffect::KeepPanes {
                    sessions,
                    terminals,
                } => {
                    panes.borrow_mut().retain(|tab, _| match tab {
                        Tab::Session(id) => sessions.contains(id),
                        Tab::Terminal(id) => terminals.contains(id),
                        Tab::Draft { .. } | Tab::Viewer(_) => true,
                    });
                    let gone = gallery
                        .peek()
                        .store
                        .sessions()
                        .any(|id| !sessions.contains(id));
                    if gone {
                        let mut gallery = gallery;
                        gallery.write().store.retain(|id| sessions.contains(id));
                    }
                }
                _ => {}
            })
        }
    });

    // Run an action where the keys are now: what a binding runs, and what a
    // click on a row's or a tab's menu runs. `key` is the key that ran it,
    // which some actions pass on; a click has none.
    let perform = {
        let active_pane = active_pane.clone();
        let mut submit = submit.clone();
        let hub = hub.clone();
        move |action: Action, key: Option<Key>| {
            let area = app.focus();
            let pane = active_pane().filter(|_| matches!(area, Area::Console | Area::Prompt));
            let nvim_insert = nvim
                .peek()
                .as_ref()
                .is_some_and(|n| n.state().mode().starts_with("insert"));
            let list = app.tree.peek().rows.clone();
            let cur = app.tree.peek().selected.clone();
            // From the sidebar an action is on the selected row; from elsewhere,
            // on what the console shows.
            let on_list = matches!(area, Area::Sidebar | Area::Details);
            let scroll = |by: &str| {
                document::eval(&format!(
                    "const b = document.querySelector('.details-scroll'); if (b) b.scrollBy(0, {by});"
                ));
            };
            // In the console, the scroll actions move its view back through
            // what scrolled off instead.
            let on_console = area == Area::Console && pane.is_some();
            let half = (console_size.peek().rows / 2).max(1) as i32;
            let mut console_scroll = |lines: i32| {
                if let Some(p) = &pane {
                    p.wheel(lines, 0, 0);
                    *frame.write() += 1;
                }
            };
            let thing = app.active();
            // The focused window's viewer takes the viewer's actions.
            if area == Area::Console
                && let Some(Tab::Viewer(_)) = &thing
                && let Some(v) = viewers::ViewerAction::of(&action)
                && let Some(w) = app.content.peek().windows.focused_window()
            {
                let seq = viewer_command.peek().as_ref().map_or(1, |(_, c)| c.seq + 1);
                viewer_command.set(Some((w, viewers::Command { seq, action: v })));
                return;
            }
            match action {
                Action::None => {}
                Action::KeysHelp => {
                    app.dispatch(Msg::KeysHelpOpen(route::help_contexts(
                        area,
                        thing.as_ref(),
                    )));
                }
                Action::ViewerZoomIn
                | Action::ViewerZoomOut
                | Action::ViewerZoomFit
                | Action::ViewerPageNext
                | Action::ViewerPagePrev
                | Action::ViewerPlay
                | Action::ViewerBack
                | Action::ViewerForward
                | Action::ViewerCopyPath => {}
                Action::WindowLeft
                | Action::WindowDown
                | Action::WindowUp
                | Action::WindowRight
                | Action::WindowNext
                | Action::WindowPrev
                | Action::WindowTop
                | Action::WindowBottom
                | Action::WindowBack
                | Action::WindowSplit
                | Action::WindowVsplit
                | Action::WindowNew
                | Action::WindowQuit
                | Action::WindowOnly
                | Action::WindowMoveLeft
                | Action::WindowMoveDown
                | Action::WindowMoveUp
                | Action::WindowMoveRight
                | Action::WindowExchange
                | Action::WindowRotate
                | Action::WindowEqualize
                | Action::WindowWider
                | Action::WindowNarrower
                | Action::WindowTaller
                | Action::WindowShorter
                | Action::WindowMaximizeWidth
                | Action::WindowMaximizeHeight
                | Action::WindowToTab
                | Action::TabGo(_)
                | Action::TabLast
                | Action::TabNext
                | Action::TabPrev
                | Action::TabClose => app.dispatch(Msg::Window(action)),
                // The key pressed is Neovim's while it shows its completions
                // or its command line, as Tab is there, and while the prompt
                // has no agent to send it to.
                Action::ConsoleSend(None)
                    if area == Area::Prompt
                        && let Some(nv) = nvim.peek().clone().filter(|n| {
                            let s = n.state();
                            pane.is_none() || s.popup().is_some() || s.mode().starts_with("cmdline")
                        })
                        && let Some(n) = key.as_ref().and_then(notation) =>
                {
                    spawn(async move {
                        let _ = nv.input(&n).await;
                    });
                }
                Action::ConsoleSend(chord) => {
                    if let Some(p) = &pane
                        && let Some(k) = chord.map(|c| c.key()).or(key)
                    {
                        p.key(&k);
                        // Control keys fill the input too: Up brings back an
                        // earlier prompt, Ctrl-_ undoes a clear.
                        app.dispatch(Msg::ConsoleTyped);
                    }
                }
                Action::ConsoleSendNext => app.dispatch(Msg::ConsoleSendNext),
                Action::ConsolePassEveryKey => {
                    if pane.is_some() {
                        app.dispatch(Msg::ConsolePassEveryKey);
                    }
                }
                Action::PromptFocus
                | Action::PromptWrite
                | Action::PromptSend
                | Action::PromptImages
                | Action::PromptTakeSuggestion
                | Action::HistoryPrev
                | Action::HistoryNext
                    if !shows_prompt(thing.as_ref()) => {}
                Action::PromptFocus => app.dispatch(Msg::Focus(Area::Prompt)),
                Action::PromptWrite => {
                    app.dispatch(Msg::Focus(Area::Prompt));
                    // The same key starts writing there, as in Vim.
                    if !nvim_insert
                        && let Some(n) = key.as_ref().and_then(notation)
                        && let Some(nv) = nvim.peek().clone()
                    {
                        spawn(async move {
                            let _ = nv.input(&n).await;
                        });
                    }
                }
                Action::PromptSend => submit(),
                Action::PromptTakeSuggestion => {
                    let Some(nv) = nvim.peek().clone() else {
                        return;
                    };
                    let key = key.unwrap_or_else(|| Key::new("Tab"));
                    let neovim_owns = {
                        let s = nv.state();
                        s.popup().is_some() || s.mode().starts_with("cmdline")
                    };
                    let tab = app.active();
                    let hands_back = ANSWERS.load(Ordering::Relaxed)
                        && matches!(&tab, Some(Tab::Session(id))
                            if nav::find(&tree.peek(), id).is_some_and(|(_, s)| s.adroit_editor));
                    let no_images = shown_images.peek().is_empty();
                    let pane = pane.clone();
                    spawn(async move {
                        let empty = no_images && nv.text().await.is_ok_and(|t| t.trim().is_empty());
                        let goes = PromptTab {
                            neovim_owns,
                            session: pane.is_some(),
                            empty,
                            hands_back,
                        }
                        .goes();
                        let (TabGoes::Session | TabGoes::SessionThenBack, Some(p)) = (&goes, pane)
                        else {
                            if let Some(n) = notation(&key) {
                                let _ = nv.input(&n).await;
                            }
                            return;
                        };
                        // Tab may type into Claude Code's input: an accepted
                        // suggestion lands there.
                        if let Some(tab @ Tab::Session(_)) = tab {
                            app.dispatch(Msg::InputMayHold(tab));
                        }
                        let before = p.screen();
                        p.key(&key);
                        if goes == TabGoes::SessionThenBack {
                            // Claude Code repaints the input once it takes the
                            // suggestion, from faint to plain, or shows a hint
                            // when it has none. Keys reach it in order, but
                            // whether Ctrl-g sees the text Tab put in rests on
                            // how it batches its updates; the repaint does not.
                            repainted(&p, &before).await;
                            app.dispatch(Msg::AskInput);
                        }
                    });
                }
                Action::PromptImages => {
                    if app.text_goes() == Typed::Prompt {
                        app.dispatch(Msg::Focus(Area::Prompt));
                        selected_image.set(shown_images.peek().first().cloned());
                    }
                }
                Action::HistoryPrev | Action::HistoryNext => {
                    // In the prompt the same keys choose among Neovim's
                    // completions and its command-line history.
                    let nv = nvim.peek().clone();
                    if area == Area::Prompt
                        && let Some(nv) = nv.filter(|n| {
                            let s = n.state();
                            s.popup().is_some() || s.mode().starts_with("cmdline")
                        })
                        && let Some(n) = key.as_ref().and_then(notation)
                    {
                        spawn(async move {
                            let _ = nv.input(&n).await;
                        });
                        return;
                    }
                    let tab = app.active();
                    let project = match &tab {
                        Some(Tab::Session(id)) => {
                            nav::find(&tree.peek(), id).map(|(_, s)| PathBuf::from(&s.cwd))
                        }
                        Some(Tab::Draft { worktree, .. }) => Some(PathBuf::from(worktree)),
                        _ => None,
                    };
                    let agent = draft_agent::for_tab(
                        &tree.peek(),
                        tab.as_ref(),
                        &app.content.peek().drafts.options,
                        app.content
                            .peek()
                            .drafts
                            .last_agent
                            .unwrap_or(settings::current().agent),
                    );
                    let back = action == Action::HistoryPrev;
                    spawn(async move {
                        let sent = match project {
                            Some(dir) => tokio::task::spawn_blocking(move || {
                                agents::adapter(agent).history(&dir)
                            })
                            .await
                            .unwrap_or_default(),
                            None => Vec::new(),
                        };
                        let _ = prompts.send(Op::Recall(tab, back, sent));
                    });
                    app.dispatch(Msg::Focus(Area::Prompt));
                    start_writing(nvim);
                }
                Action::SessionResume => app.dispatch(Msg::ResumeOpen),
                Action::SessionAgentNext => switch_agent(true),
                Action::SessionAgentPrev => switch_agent(false),
                Action::SessionStop => app.dispatch(Msg::ConfirmStop { selection: on_list }),
                Action::SessionRename => {
                    let chosen = match &cur {
                        Some(Item::Session(id) | Item::Waiting(id)) if on_list => Some(id.clone()),
                        _ => match &app.active() {
                            Some(Tab::Session(id)) => Some(id.clone()),
                            _ => None,
                        },
                    };
                    if let Some(id) = chosen {
                        let place = if on_list {
                            RenamePlace::Sidebar
                        } else {
                            RenamePlace::Tab
                        };
                        app.dispatch(Msg::RenameStart { id, place });
                    }
                }
                Action::ScrollDown if on_console => console_scroll(-1),
                Action::ScrollUp if on_console => console_scroll(1),
                Action::ScrollHalfDown if on_console => console_scroll(-half),
                Action::ScrollHalfUp if on_console => console_scroll(half),
                Action::ScrollTop if on_console => console_scroll(i32::MAX),
                Action::ScrollBottom if on_console => console_scroll(i32::MIN),
                Action::ScrollDown => scroll("17"),
                Action::ScrollUp => scroll("-17"),
                Action::ScrollHalfDown => scroll("b.clientHeight / 2"),
                Action::ScrollHalfUp => scroll("-b.clientHeight / 2"),
                Action::ScrollTop => scroll("-b.scrollHeight"),
                Action::ScrollBottom => scroll("b.scrollHeight"),
                Action::CursorDown
                | Action::CursorUp
                | Action::CursorFirst
                | Action::CursorLast
                | Action::CursorGo
                | Action::CursorSplit
                | Action::CursorVsplit
                | Action::CursorTab
                | Action::CursorClose
                | Action::CursorOpen
                | Action::CursorShow
                | Action::CursorFold
                | Action::CursorCopy
                | Action::CursorReveal
                    if area == Area::Details =>
                {
                    // In the details panel, the cursor moves through a list
                    // shown and scrolls anything else.
                    let key = match action {
                        Action::CursorDown => Some(details::Key::Down),
                        Action::CursorUp => Some(details::Key::Up),
                        Action::CursorFirst => Some(details::Key::First),
                        Action::CursorLast => Some(details::Key::Last),
                        Action::CursorGo | Action::CursorOpen | Action::CursorShow => {
                            Some(details::Key::Open)
                        }
                        Action::CursorFold => Some(details::Key::Fold),
                        Action::CursorCopy => Some(details::Key::Copy),
                        Action::CursorReveal => Some(details::Key::Reveal),
                        _ => None,
                    };
                    if let Some(key) = key {
                        app.dispatch(Msg::DetailsKey(key));
                    }
                }
                Action::CursorDown => app.dispatch(Msg::TreeKey(tree_view::Key::Down)),
                Action::CursorUp => app.dispatch(Msg::TreeKey(tree_view::Key::Up)),
                Action::CursorFirst => app.dispatch(Msg::TreeKey(tree_view::Key::First)),
                Action::CursorLast => app.dispatch(Msg::TreeKey(tree_view::Key::Last)),
                Action::CursorGo
                | Action::CursorOpen
                | Action::CursorSplit
                | Action::CursorVsplit
                | Action::CursorTab => {
                    if let Some(item) = cur {
                        let how = match action {
                            Action::CursorGo => Opening::Existing,
                            Action::CursorSplit => Opening::Split(Axis::Column),
                            Action::CursorVsplit => Opening::Split(Axis::Row),
                            Action::CursorTab => Opening::Tab,
                            _ => Opening::Here,
                        };
                        open(item, Some(how), true);
                    }
                }
                Action::CursorShow => {
                    if let Some(item) = cur {
                        open(item, Some(Opening::Here), false);
                    }
                }
                Action::CursorFold => app.dispatch(Msg::TreeKey(tree_view::Key::Fold)),
                Action::CursorClose => app.dispatch(Msg::Window(action)),
                Action::CursorCopy => {
                    let copied = cur.as_ref().and_then(|item| {
                        nav::copied(item, &app.tree.peek().narrowed, &terminals.peek())
                    });
                    let (text, what) = match copied {
                        Some(nav::Copied::Path(p)) => (p.display().to_string(), "path"),
                        Some(nav::Copied::Name(n)) => (n, "name"),
                        None => return,
                    };
                    app.notice(match copy_to_clipboard(&text) {
                        Ok(()) => format!("Copied the {what} {text}"),
                        Err(e) => format!("cannot copy: {e}"),
                    });
                }
                // Only the overview's files are revealed.
                Action::CursorReveal => {}
                Action::WaitingNext | Action::WaitingPrev => {
                    let current = match &cur {
                        Some(Item::Waiting(id)) | Some(Item::Session(id)) => Some(id.as_str()),
                        _ => None,
                    };
                    let forward = action == Action::WaitingNext;
                    let next = nav::next_waiting(&app.tree.peek().waiting, current, forward);
                    if let Some(id) = next {
                        open(Item::Waiting(id), None, false);
                        // It waits on an answer, given in the console.
                        app.dispatch(Msg::Focus(Area::Console));
                    }
                }
                Action::SessionPick => app.dispatch(Msg::PickerOpen),
                Action::SessionScratch => {
                    app.dispatch(Msg::PickerClose);
                    app.dispatch(Msg::Window(action));
                }
                Action::SessionNext
                | Action::SessionPrev
                | Action::WorkspaceNext
                | Action::WorkspacePrev
                | Action::SessionNew
                | Action::ShellOpen
                | Action::NeovimOpen => app.dispatch(Msg::Window(action)),
                // On a shell's row, that shell closes, whichever tab is shown.
                Action::TerminalStop
                    if let Some(Item::Terminal(id)) = cur.as_ref().filter(|_| on_list) =>
                {
                    app.dispatch(Msg::TreeSelect(nav::row_above(&list, &Item::Terminal(*id))));
                    end_shell(&hub, *id);
                    close_shell(*id, &hub.panes, app);
                }
                Action::TerminalStop => {
                    let current_tab = app.active();
                    if let Some(Tab::Terminal(id)) = current_tab {
                        end_shell(&hub, id);
                        close_shell(id, &hub.panes, app);
                    }
                }
                Action::Refresh => app.dispatch(Msg::Relist),
                Action::DetailsOverview | Action::DetailsChanges | Action::DetailsLog => {
                    let s = match action {
                        Action::DetailsChanges => Sheet::Changes,
                        Action::DetailsLog => Sheet::Log,
                        _ => Sheet::Overview,
                    };
                    app.dispatch(Msg::DetailsSheet(s));
                    app.dispatch(Msg::PanelShow(Side::Details));
                }
                Action::WorktreesToggle => app.dispatch(Msg::TreeEveryWorktree),
                Action::SettingsOpen => settings_window::open(),
                Action::DaemonRestart => app.dispatch(Msg::Confirm(Box::new(Ask::RestartDaemon))),
                Action::WindowClose => dioxus::desktop::window().close(),
                Action::TreeToggle => toggle_panel(Side::Sidebar),
                Action::DetailsToggle => toggle_panel(Side::Details),
                Action::ZoomIn => app.dispatch(Msg::Zoom(zoom::Step::In)),
                Action::ZoomOut => app.dispatch(Msg::Zoom(zoom::Step::Out)),
                Action::ZoomReset => app.dispatch(Msg::Zoom(zoom::Step::Reset)),
                Action::Filter => {
                    app.dispatch(Msg::Focus(Area::Sidebar));
                    app.dispatch(Msg::TreeFilter(String::new()));
                    focus_filter(true);
                }
            }
        }
    };

    // What a menu entry runs, once the view state has selected what it was
    // opened on.
    use_hook({
        let perform = RefCell::new(perform.clone());
        move || {
            app.hand_perform(move |action, key| {
                if let Ok(mut perform) = perform.try_borrow_mut() {
                    perform(action, key);
                }
            })
        }
    });
    // Run an action from a button, where the keys are now.
    let run_button = {
        let mut perform = perform.clone();
        move |action: Action| perform(action, None)
    };

    // The menu bar shows the Command keys in force, and the page leaves those
    // keys to it, so a key runs once, from the menu.
    use_effect(move || {
        let map = keymap();
        menu_bar::show_shortcuts(&map);
        let held = serde_json::to_string(&menu_bar::left_to_menu(&map)).unwrap_or_default();
        document::eval(&format!("window.adroitHeld = {held};"));
    });
    // The page learns which Option keys count as Alt, now and as the settings
    // change.
    use_hook(|| {
        spawn(async move {
            let mut changed = settings::CHANGED.subscribe();
            loop {
                let as_alt = settings::current().console.option_as_alt.name();
                document::eval(&format!("window.adroitOptionAsAlt = '{as_alt}';"));
                if changed.recv().await.is_err() {
                    break;
                }
            }
        });
    });
    // The window made key again gives the keys to its page, whatever in the
    // window took them meanwhile: AppKit sends them to the first responder,
    // and with anything but the page there no key reaches the window.
    dioxus::desktop::use_wry_event_handler({
        let desktop = dioxus::desktop::window();
        move |event, _| {
            use dioxus::desktop::tao::event::Event;
            if let Event::WindowEvent {
                window_id,
                event: dioxus::desktop::WindowEvent::Focused(on),
                ..
            } = event
                && *window_id == desktop.window.id()
            {
                window_focused.set(*on);
                if *on {
                    keys::trace(|| "window made key: the page takes the keys");
                    let _ = desktop.webview.focus();
                }
            }
        }
    });
    // Run what a menu bar entry names where the keys are now, as its key
    // would: not behind the picker, a dialog or a name being typed, which hold
    // the keys. The entries act on this window whichever of adroit's is in
    // front, and bring it forward to show what they did.
    dioxus::desktop::use_muda_event_handler({
        let mut perform = perform.clone();
        let desktop = dioxus::desktop::window();
        move |event| {
            let Some(action) = menu_bar::action_of(&event.id().0) else {
                return;
            };
            // Closing acts on the window in front, as macOS's own Close does,
            // so it does not reach behind Settings to a tab.
            if matches!(action, Action::TabClose | Action::WindowClose)
                && (settings_window::close_in_front() || image_window::close_in_front())
            {
                return;
            }
            if overlay::holds_menu_bar(&app.held()) {
                return;
            }
            if action != Action::SettingsOpen {
                desktop.window.set_focus();
            }
            app.dispatch(Msg::KeysHelpClose);
            app.dispatch(Msg::MenuClose);
            app.dispatch(Msg::SequenceDropped);
            perform(action, None);
        }
    });

    let handle_key = {
        let active_pane = active_pane.clone();
        move |key: Key| {
            if key.key == "Process"
                || key.key == "Dead"
                || key.key == "Unidentified"
                || keys::held_alone(&key)
            {
                return;
            }
            // What is drawn over the window holds the keys while it is.
            if let Some(holder) = app.held().first().copied() {
                keys::trace(|| format!("to the {holder:?}"));
                app.dispatch(Msg::HeldKey(key));
                return;
            }
            let area = app.focus();
            if area == Area::Prompt && app.text_goes() == Typed::Prompt {
                let images = shown_images.peek();
                let chosen = selected_image.peek().clone();
                if let Some(action) = pasted::image_key(&images, chosen.as_ref(), &key) {
                    match action {
                        pasted::ImageKey::Select(path) => selected_image.set(path),
                        pasted::ImageKey::Remove(path) => {
                            let _ = prompts.send(Op::RemoveImage(path));
                        }
                    }
                    return;
                }
                selected_image.set(None);
            }
            // What the window knows of its programs, which the view state
            // routes the key by.
            let pane = active_pane();
            let reader = reader.peek().clone().map(|r| {
                let screen = r.state().screen();
                ReaderAt {
                    normal: r.state().mode() == "normal",
                    edge: Edge {
                        line: reader_view.peek().0 + screen.cursor.map_or(0, |c| c.row),
                        lines: read_rows.peek().0,
                        page: screen.rows,
                    },
                }
            });
            let probe = Probe {
                pane: pane.is_some(),
                scrolls_itself: pane.as_ref().is_some_and(|p| p.takes_mouse()),
                reader,
                inserting: nvim
                    .peek()
                    .as_ref()
                    .is_some_and(|n| n.state().mode().starts_with("insert")),
            };
            keys::trace(|| format!("area={area:?} {probe:?}"));
            app.dispatch(Msg::Key {
                key,
                probe: Box::new(probe),
            });
        }
    };

    // Whether the page has the keys, as it last told.
    let page_focused = use_hook(|| Rc::new(Cell::new(true)));
    // Keys, text, pastes and input-method commits from the hidden text area.
    // Keys with a modifier and keys that type nothing come as keys; plain
    // characters come as text.
    use_hook({
        let mut handle_key = handle_key.clone();
        let page_focused = page_focused.clone();
        move || {
            spawn(async move {
                let run = || {
                    document::eval(&format!(
                        "window.adroitHeld ??= {MENU_KEYS:?};\nwindow.adroitDebugKeys = {};\n{INPUT}",
                        *keys::TRACED
                    ))
                };
                let mut input = run();
                loop {
                    let message = match keys::after(input.recv::<keys::Message>().await) {
                        keys::Next::Handle(message) => message,
                        keys::Next::Skip => {
                            keys::trace(|| "passed over a message that cannot be read");
                            continue;
                        }
                        keys::Next::Restart => {
                            keys::trace(|| "the keys were lost: running the script again");
                            // A script that cannot keep its channel is not run
                            // again at once, over and over.
                            tokio::time::sleep(RESTART_INPUT_AFTER).await;
                            input = run();
                            continue;
                        }
                    };
                    if let Some(key) = message.key() {
                        keys::trace(|| format!("received {key:?}"));
                        handle_key(key);
                        continue;
                    }
                    match message {
                        keys::Message::Trace { text } => keys::trace(|| format!("js {text}")),
                        keys::Message::Focused { on } => page_focused.set(on),
                        keys::Message::Paste { text } => {
                            keys::trace(|| format!("text {text:?} pasted=true"));
                            app.dispatch(Msg::Text { text, pasted: true });
                        }
                        keys::Message::Image { mime, data } => {
                            app.dispatch(Msg::ImagePasted { mime, data });
                        }
                        keys::Message::Text { text } if text.chars().count() == 1 => {
                            handle_key(Key::new(&text));
                        }
                        keys::Message::Text { text } => {
                            keys::trace(|| format!("text {text:?} pasted=false"));
                            app.dispatch(Msg::Text {
                                text,
                                pasted: false,
                            });
                        }
                        keys::Message::Key { .. } | keys::Message::Lost => {}
                    }
                }
            });
        }
    });
    // A window that has the keys while its page has not, past a moment, gives
    // them back to the page: nothing else in the window uses them, and with
    // them elsewhere every key is lost while the window looks as it should.
    use_hook(move || {
        let desktop = dioxus::desktop::window();
        spawn(async move {
            let mut watch = keys::Watch::default();
            loop {
                tokio::time::sleep(LOOK_FOR_KEYS_EVERY).await;
                let key = desktop.window.is_focused();
                if watch.look(key, page_focused.get(), desktop.webview.is_devtools_open()) {
                    keys::trace(|| "the page lost the keys of the window: giving them back");
                    let _ = desktop.webview.focus();
                }
            }
        });
    });

    let _ = attached();
    let area = focus();
    let cw = cell_width();
    let has_pane = active_pane().is_some();
    let console_background = hub.palette.read().background.hex();
    // The theme's colours and the console's font, over the stylesheet's own.
    let root_style = {
        let s = font();
        format!(
            ":root{{{theme}--term-font:{family};--term-size:{size}px;--term-weight:{weight};--cell-h:{h}px}}",
            theme = themes::css(&themes::window(&hub.palette.read())),
            family = s.console_font(),
            size = s.font_size,
            weight = s.font_weight,
            h = cell_height_now(),
        )
    };
    // The images on the console's screen, which is the last of the lines the
    // reader holds, moved as the reader is scrolled over them.
    let (reading, passing, dialog) = {
        let console = app.console.read();
        (console.reading, console.direct, console.dialog)
    };
    let reader_pictures: Vec<adroit_term::Picture> = if reading {
        let _ = reader_frame();
        let _ = frame();
        let (top, left) = reader_view();
        let (held, rows) = read_rows();
        let down = held.saturating_sub(rows) as f32 - top as f32;
        active_pane()
            .map(|p| p.pictures())
            .unwrap_or_default()
            .into_iter()
            .map(|mut p| {
                p.dst[0] -= left as f32;
                p.dst[1] += down;
                p
            })
            .filter(|p| p.dst[0] + p.dst[2] > 0.0)
            .collect()
    } else {
        Vec::new()
    };
    let t = tree.read().clone();
    let current = app.content.read().described().cloned();
    let workspace = current
        .as_ref()
        .and_then(|p| t.workspace_of(&p.worktree))
        .map(|w| (w.name.clone(), w.root.clone(), w.scratch));
    // A workspace outside git may still be a jj repository, which the details
    // panel reads through jj.
    let outside_git = current.as_ref().is_some_and(|p| {
        t.workspace_of(&p.worktree).is_some_and(|w| !w.git) && !Repo::open(&p.worktree).is_jj()
    });
    let active_tab = active();
    let current_agent = draft_agent::for_tab(
        &t,
        active_tab.as_ref(),
        &app.content.read().drafts.options,
        app.content
            .read()
            .drafts
            .last_agent
            .unwrap_or(font.read().agent),
    );
    let live_input_ms = active_tab.as_ref().and_then(|tab| {
        hub.panes
            .borrow()
            .get(tab)
            .and_then(|pane| pane.last_input_ms())
    });
    let target = match &active_tab {
        Some(Tab::Session(id)) => nav::find(&t, id)
            .map(|(_, s)| s.name.clone())
            .unwrap_or_else(|| id.clone()),
        Some(Tab::Terminal(n)) => terminals
            .read()
            .iter()
            .find(|s| s.id == *n)
            .map(|s| s.name.clone())
            .unwrap_or_default(),
        Some(Tab::Draft { .. }) => "new session".into(),
        Some(Tab::Viewer(path)) => file_name(path),
        None => "new scratch session".into(),
    };
    let prompt_shown = shows_prompt(active_tab.as_ref());
    let composer_room = if prompt_shown {
        composer_height() + 22.0
    } else {
        0.0
    };
    let mode = prompt_mode();
    // Whether what has the keys takes text, which an input method may compose.
    let session_shown = matches!(active_tab, Some(Tab::Session(_)));
    let in_dialog = session_shown && dialog;
    let answering = match &active_tab {
        Some(Tab::Session(id)) => {
            in_dialog || nav::find(&t, id).is_some_and(|(_, s)| s.state.takes_every_key())
        }
        _ => false,
    };
    let held = overlay::holding(&app.overlays.read(), &app.picker.read(), &app.chrome.read());
    let takes_text = match area {
        // A dialog's text is typed in its own field.
        _ if overlay::takes_text_away(&held) => false,
        Area::Console => passing || answering || !session_shown,
        Area::Prompt => {
            answering || (selected_image().is_none() && matches!(mode.0, "insert" | "cmd"))
        }
        Area::Sidebar | Area::Details => false,
    };
    let reader_mode = reading.then(|| {
        let _ = reader_frame();
        reader()
            .map(|r| r.state().mode().to_string())
            .unwrap_or_default()
    });
    // The picker holds every key while it is open, the console's too.
    let (footer_mode, footer_class) = Footing {
        picking: app.picker.read().open.is_some(),
        area,
        shown: active_tab.as_ref(),
        direct: passing,
        answering,
        reading: reader_mode.as_deref(),
        prompt: mode,
    }
    .mode();
    // What the window says about its keys, from the bindings in force.
    let km = keymap.read().clone();
    let k_moves = km.keys_for_all(
        Context::Global,
        &[
            Action::WindowLeft,
            Action::WindowDown,
            Action::WindowUp,
            Action::WindowRight,
        ],
    );
    let k_waiting_prev = km.keys_for(Context::Tree, &Action::WaitingPrev);
    let k_waiting_next = km.keys_for(Context::Tree, &Action::WaitingNext);
    // The footer names the keys that work where the keys are now.
    let k_waiting_next_here = km.keys_in(route::footer_contexts(area), &Action::WaitingNext);
    let k_back = km
        .keys_in(&[Context::Details, Context::Tree], &Action::CursorFold)
        .unwrap_or_default();
    let k_send = km.keys_for(Context::Prompt, &Action::PromptSend);
    let k_write = km.keys_for(Context::Console, &Action::PromptWrite);
    let k_escape = km.keys_for(
        Context::Console,
        &Action::ConsoleSend(Chord::parse("Escape").ok()),
    );
    let k_every = km.keys_for(Context::Console, &Action::ConsolePassEveryKey);
    let k_ctrl_q = km.keys_for(
        Context::Global,
        &Action::ConsoleSend(Chord::parse("Ctrl+q").ok()),
    );

    let focused_body = rsx! {
        div {
            class: "console",
            style: "--composer:{composer_room}px;--console-bg:{console_background};background:{console_background}",
            onresize: move |e: Event<ResizeData>| {
                e.stop_propagation();
                if let Ok(sz) = e.data().get_content_box_size() {
                    console_box.set((sz.width, sz.height));
                }
            },
            match (&active_tab, has_pane) {
                (Some(tab @ Tab::Draft { worktree, .. }), _) => rsx! {
                    DraftForm {
                        worktree: worktree.clone(),
                        agent: current_agent,
                        options: app.content.read().drafts.options.get(tab).cloned().unwrap_or_default(),
                        keymap: km.clone(),
                        onagent: move |a| {
                            choose_agent(app.active(), a);
                            app.dispatch(Msg::Focus(Area::Prompt));
                            let writing = nvim
                                .peek()
                                .as_ref()
                                .is_some_and(|n| n.state().mode().starts_with("insert"));
                            if !writing {
                                start_writing(nvim);
                            }
                        },
                        onoptions: {
                            let tab = tab.clone();
                            move |options| app.dispatch(Msg::DraftOptions { tab: Some(tab.clone()), options })
                        },
                    }
                },
                (Some(_), true) if reading => rsx! {
                    ReaderScreen { reader, reader_frame, cell_width: cw, focused: area == Area::Console, pictures: reader_pictures.clone(), onscroll: { let mut scroll = scroll_reader.clone(); move |lines: i32| scroll(lines) }, onclick: { let click = click_reader.clone(); move |at| click(at) } }
                },
                (Some(tab), true) => rsx! {
                    ConsoleScreen { tab: tab.clone(), frame, cell_width: cw, focused: area == Area::Console }
                },
                (Some(_), false) => rsx! {
                    Starting {}
                },
                (None, _) => rsx! {
                    NothingOpen {
                        agent: current_agent,
                        options: app.content.read().drafts.next.clone().unwrap_or_default(),
                        keymap: km.clone(),
                        onpick: move |_| app.dispatch(Msg::PickerOpen),
                        onagent: move |a| choose_agent(None, a),
                        onoptions: move |options| app.dispatch(Msg::DraftOptions { tab: None, options }),
                    }
                },
            }
            if passing && area == Area::Console {
                div { class: "direct-banner",
                    span { class: "direct-banner-title", "Passing every key" }
                    span { "to the session. " kbd { "Esc" } " stops, " kbd { "Ctrl-c" } " sends Esc." }
                }
            }
            if prompt_shown {
                Composer {
                    nvim,
                    prompt_frame,
                    cell_width: cw,
                    focused: area == Area::Prompt,
                    target: target.clone(),
                    composer_height,
                    prompt_width,
                    onpick: move |_| {
                        selected_image.set(None);
                        app.dispatch(Msg::Focus(Area::Prompt));
                        // A click on the prompt is to write there; the
                        // first key typed would otherwise run as a command.
                        let writing = nvim
                            .peek()
                            .as_ref()
                            .is_some_and(|n| n.state().mode().starts_with("insert"));
                        if !writing {
                            start_writing(nvim);
                        }
                    },
                    send_keys: k_send.clone(),
                    view: prompt_view(),
                    rows: prompt_rows(),
                    shell: prompt_shell(),
                    answering,
                    images: shown_images(),
                    image_keys: km.keys_for(Context::Prompt, &Action::PromptImages),
                    selected_image: selected_image(),
                    onselectimage: move |path| { app.dispatch(Msg::Focus(Area::Prompt)); selected_image.set(Some(path)); },
                    onremoveimage: move |path| { let _ = prompts.send(Op::RemoveImage(path)); },
                }
            }
        }
    };

    rsx! {
        style { "{root_style}" }
        div { class: "app",
            textarea { id: "adroit-keys", autofocus: true, readonly: !takes_text }
            settings_window::SettingsLoadError {}
            Header { ontoggle: toggle_panel }
            div {
                class: match (app.chrome.read().panels.drag.is_some(), app.content.read().drag.as_ref().map(|d| d.border.axis)) {
                    (_, Some(Axis::Column)) => "app-body dragging rows",
                    (true, _) | (_, Some(Axis::Row)) => "app-body dragging",
                    _ => "app-body",
                },
                onresize: move |e: Event<ResizeData>| {
                    if let Ok(sz) = e.data().get_border_box_size() {
                        app.dispatch(Msg::BodyResized(sz.width));
                    }
                },
                onmousemove: move |e: Event<MouseData>| {
                    let dragging = app.content.peek().drag.as_ref().map(|d| d.border.axis);
                    if let Some(axis) = dragging {
                        let at = e.client_coordinates();
                        let pos = if axis == Axis::Row { at.x } else { at.y };
                        app.dispatch(Msg::WindowDragMove(pos));
                    }
                    shell::panels::pointer_moved(app, e.client_coordinates().x);
                },
                onmouseup: move |_| {
                    if app.content.peek().drag.is_some() {
                        app.dispatch(Msg::WindowDragEnd);
                    }
                    shell::panels::pointer_released(app);
                },
                onmouseleave: move |_| {
                    if app.content.peek().drag.is_some() {
                        app.dispatch(Msg::WindowDragEnd);
                    }
                    shell::panels::pointer_released(app);
                },
                TreePanel {
                    focused: area == Area::Sidebar,
                    context: RowContext {
                        completions: completions.into(),
                        drafted: drafted.into(),
                    },
                    on: RowEvents {
                        onopen: EventHandler::new(move |item| {
                            open(item, Some(Opening::Existing), false);
                        }),
                        onstart: EventHandler::new(move |item| {
                            app.dispatch(Msg::MenuRun { target: Target::Row(item), action: Action::SessionNew });
                        }),
                    },
                    field: filter_field,
                    waiting_keys: (k_waiting_prev.clone(), k_waiting_next.clone()),
                    ontaken: move |_| app.dispatch(Msg::Focus(Area::Sidebar)),
                    onfilter: focus_filter,
                    onworktrees: {
                        let run = run_button.clone();
                        move |_| {
                            let mut run = run.clone();
                            run(Action::WorktreesToggle);
                        }
                    },
                    onpick: move |_| app.dispatch(Msg::PickerOpen),
                }
                if layout().show_sidebar {
                    Splitter { side: Side::Sidebar }
                }
                ContentPanel {
                    focused: matches!(area, Area::Console | Area::Prompt),
                    focused_body,
                    viewer_command,
                    completions,
                    drafted,
                    boxes: win_boxes,
                    frame,
                    cell_width: cw,
                    background: console_background.clone(),
                    ontabclose: move |target| app.dispatch(Msg::MenuRun { target, action: Action::TabClose }),
                }
                if layout().show_details {
                    Splitter { side: Side::Details }
                }
                div { class: if area == Area::Details { "app-panel details focused" } else { "app-panel details" },
                    hidden: !layout().show_details,
                    style: "width:{app.chrome.read().panels.drawn().1}px",
                    onmousedown: move |_| app.dispatch(Msg::Focus(Area::Details)),
                    details::DetailsPanel {
                        worktree: current.as_ref().map(|p| p.worktree.clone()),
                        workspace_root: workspace.as_ref().map(|(_, root, _)| root.clone()),
                        kind: if outside_git { details::Worktree::OutsideGit } else { details::Worktree::Repository },
                        live_input_ms,
                        agent: current_agent,
                        usage_settings: font.read().usage,
                        sheet_keys: Sheet::ALL.map(|s| km.keys_for(Context::Global, &details::sheet_action(s)).unwrap_or_default()).to_vec(),
                        back_keys: k_back.clone(),
                        on_kill: move |(session, process): (Session, crate::processes::Process)| {
                            if let Some(ask) = Ask::kill_process(Some(&session), Some(&Entry::Process(process))) {
                                app.dispatch(Msg::Confirm(Box::new(ask)));
                            }
                        },
                        on_menu: move |m: details::LinkMenu| {
                            app.dispatch(Msg::MenuOpen { x: m.x, y: m.y, target: Target::Link { at: m.at, file: m.file } });
                        },
                    }
                }
            }
            DaemonBanner {}
            StatusBar { mode: footer_mode, class: footer_class, dialog: in_dialog,
                if let Some(k) = k_moves.clone() {
                    span { class: "kbd-list", Keys { keys: k } " move" }
                }
                if let Some(k) = k_waiting_next_here.clone() {
                    span { class: "kbd-list", Keys { keys: k } " next waiting" }
                }
                if area == Area::Sidebar && app.tree.read().filtering {
                    span { class: "kbd-list", kbd { "Enter" } " keep the filter" }
                    span { class: "kbd-list", kbd { "Esc" } " clear it" }
                }
                if area == Area::Prompt && in_dialog {
                    span { class: "kbd-list", "every key goes to the session's dialog" }
                    span { class: "kbd-list", "the prompt is back once it closes" }
                } else if area == Area::Console && in_dialog && !passing {
                    span { class: "kbd-list", "every key goes to the session's dialog" }
                    span { class: "kbd-list", "the window's moves leave" }
                } else if area == Area::Prompt && answering {
                    span { class: "kbd-list", "every key answers the session" }
                    span { class: "kbd-list", "the prompt is back once it is answered" }
                } else if area == Area::Console && answering && !passing {
                    span { class: "kbd-list", "every key answers the session" }
                    span { class: "kbd-list", "the window's moves leave" }
                } else if area == Area::Console && passing {
                    span { class: "kbd-list", kbd { "Esc" } " stop passing keys" }
                    span { class: "kbd-list", kbd { "Ctrl-c" } " send Esc" }
                } else if area == Area::Console && session_shown && reading {
                    span { class: "kbd-list", kbd { "v" } kbd { "V" } " select" }
                    span { class: "kbd-list", kbd { "y" } " copy" }
                    span { class: "kbd-list", kbd { "/" } " search" }
                    span { class: "kbd-list", kbd { "q" } " live" }
                    if let Some(k) = k_write.clone() {
                        span { class: "kbd-list", Keys { keys: k } " write" }
                    }
                } else if area == Area::Console && session_shown {
                    span { class: "kbd-list", kbd { "j" } kbd { "k" } " read" }
                    if let Some(k) = k_write.clone() {
                        span { class: "kbd-list", Keys { keys: k } " write" }
                    }
                    if let Some(k) = k_escape.clone() {
                        span { class: "kbd-list", Keys { keys: k } " send Esc" }
                    }
                    if let Some(k) = k_every.clone() {
                        span { class: "kbd-list", Keys { keys: k } " pass every key" }
                    }
                } else if area == Area::Console {
                    if let Some(k) = k_ctrl_q.clone() {
                        span { class: "kbd-list", Keys { keys: k } " send Ctrl-q" }
                    }
                }
            }
            Picker {}
            ContextMenu {}
            KeysHelp {}
            ConfirmDialog {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn usage_depends_only_on_the_console_agent() {
        use adroit_model::Agent;

        for agent in [Agent::Claude, Agent::Codex] {
            for tab in [
                Tab::Session("session".into()),
                Tab::Draft {
                    worktree: "/project".into(),
                    id: Default::default(),
                },
            ] {
                assert!(shows_usage(Some(&tab), agent));
            }
            assert!(!shows_usage(Some(&Tab::Terminal(1)), agent));
            assert!(!shows_usage(None, agent));
        }
    }

    #[test]
    fn the_clipboard_reads_the_text_as_utf_8_whatever_the_app_s_locale() {
        let cmd = clipboard_command();
        let lc_all = cmd
            .get_envs()
            .find(|(k, _)| *k == "LC_ALL")
            .and_then(|(_, v)| v);
        assert_eq!(lc_all, Some(std::ffi::OsStr::new("en_US.UTF-8")));
    }

    #[test]
    fn the_console_s_terminal_takes_the_prompt_card_s_room_when_no_card_is_shown() {
        let with_card = console_room(600.0, Some(120.0));
        let without = console_room(600.0, None);
        assert_eq!(with_card, 600.0 - 120.0 - CONSOLE_GAP);
        assert_eq!(without, 600.0 - SCREEN_TOP);
        assert!(without > with_card + 120.0);
    }

    #[test]
    fn text_taken_from_a_session_s_input_is_counted_without_naming_its_agent() {
        assert_eq!(
            moved_notice(1),
            "moved 1 line from the session's input to the prompt"
        );
        assert_eq!(
            moved_notice(3),
            "moved 3 lines from the session's input to the prompt"
        );
    }
}
