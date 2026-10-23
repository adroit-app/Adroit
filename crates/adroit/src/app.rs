use std::cell::{Cell, RefCell};
use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::Arc;
use std::time::Duration;

use adroit_model::Session;
use adroit_nvim::notation;
use adroit_protocol::Typing;
use adroit_term::{Key, Palette, Pane, Screen, Size, Spawn};
use dioxus::prelude::*;
use tokio::sync::{mpsc, oneshot};

use crate::actions::Action;
use crate::bridge::{AppCtx, use_app};
use crate::bundled_font::{faces, use_bundled_font};
use crate::complete;
use crate::console::{Consoles, Hub, Panes, PromptNeovim, Reader, use_prompt_neovim, use_reader};
use crate::content::{
    Area, Axis, ContentPanel, Tab, TerminalAsk, WindowId, file_name, fresh_name, label_from,
    shows_prompt,
};
use crate::daemon;
use crate::details::{self, Entry, Sheet, Shot, Source};
use crate::editor;
use crate::image_window;
use crate::keymap::{Chord, Context, MENU_KEYS};
use crate::keys;
use crate::layout::Side;
use crate::menu_bar;
use crate::nav::{self, Item};
use crate::pasted;
use crate::paths::{instance_state, scratch_dir};
use crate::picker::Picker;
use crate::pictures::{self, Gallery};
use crate::programs;
use crate::prompt_tab::{PromptTab, TabGoes};
use crate::prompts::{self, Op, Prompts};
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
use adroit_agents as agents;
use adroit_core::console::reader::{self, Edge};
use adroit_core::console::route::{self, Footing, passes_enter};
use adroit_core::console::{Probe, ReaderAt};
use adroit_core::effect::Effect as CoreEffect;
use adroit_core::msg::Msg;
use adroit_core::overlay;
use adroit_core::overlay::confirm::Ask;
use adroit_core::overlay::context_menu::Target;

/// The palette of the theme `settings` name, or adroit's own when it cannot
/// be read.
fn palette_of(settings: &settings::Settings) -> Result<Palette, String> {
    themes::palette(&settings.theme, &crate::settings::themes_dir())
}

/// Open the window.
pub fn launch() {
    use dioxus::desktop::{Config as Desktop, LogicalSize, WindowBuilder};
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

/// Type `typing` into `pane`, step by step.
async fn type_into(pane: &Pane, typing: &[Typing]) {
    for step in typing {
        match step {
            Typing::Text { text } => pane.type_text(text),
            Typing::Paste { text } => pane.paste(text),
            Typing::Key { name } => {
                pane.key(&Key::new(name));
            }
            Typing::Pause { ms } => tokio::time::sleep(Duration::from_millis(*ms)).await,
        }
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
    let viewer_command = use_signal(|| None::<(WindowId, viewers::Command)>);
    // The content as the view state holds it, each part read alone.
    let active = use_memo(move || app.content.read().active().cloned());
    let focus = use_memo(move || app.content.read().focus);
    // The finished sessions the user has seen, as the tree holds them.
    let completions = use_memo(move || app.tree.read().completions.clone());
    let mut frame = use_signal(|| 0u64);
    // Moves when a pane is started, for what the console says around it.
    let mut attached = use_signal(|| 0u64);
    let mut cell_width = use_signal(|| 7.8f64);
    let mut console_size = use_signal(|| Size::new(100, 30));
    let mut console_box = use_signal(|| (0.0f64, 0.0f64));
    let composer_height = use_signal(|| 0.0f64);
    let prompt_width = use_signal(|| 0.0f64);
    // The zoom alone, which the console is measured again for.
    let zoomed = use_memo(move || app.chrome.read().zoom);
    // The settings in force, the font the console is drawn by among them,
    // and the height of a line measured in it.
    let settings = use_memo(move || app.config.read().settings.clone());
    let face = use_memo(move || {
        let s = settings.read();
        (s.font_family.clone(), s.font_size, s.font_weight)
    });
    let mut cell_height_now = use_signal(|| DEFAULT_CELL_HEIGHT);
    use_context_provider(|| CellHeight(cell_height_now));
    // The theme's palette, which a new theme replaces.
    let palette = use_signal(|| palette_of(&app.config.peek().settings).unwrap_or_default());
    // The side panels' widths and whether they are shown, as the chrome
    // holds them.
    let layout = use_memo(move || app.chrome.read().panels.layout);
    // How large each window of the content that is not focused is drawn,
    // whose program is sized to it.
    let win_boxes = use_signal(HashMap::<WindowId, (f64, f64)>::new);
    let toggle_panel = move |side: Side| app.dispatch(Msg::PanelToggle(side));
    // The tabs with something written for them kept aside.
    let drafted = use_memo(move || app.prompt.read().drafted.clone());
    let has_images = use_memo(move || !app.prompt.read().images.is_empty());
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
    // then on; the host told when the user last typed into each session,
    // and the view state which sessions are live here, as their panes exit.
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
                let mut typed = BTreeMap::new();
                let mut told_live = None;
                loop {
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

    // A `/` typed asks for what it offers again, if that is old.
    let commands_wanted = use_callback(move |()| app.dispatch(Msg::CommandsWanted));
    let prompt_neovim =
        use_prompt_neovim(app, has_images, cell_width, prompt_width, commands_wanted);
    let PromptNeovim {
        nvim,
        prompt_frame,
        prompt_view,
        prompt_rows,
        ..
    } = prompt_neovim;
    let prompts = use_hook(|| prompts::keeper(nvim, app));

    // The images the shown session's console drew, which the details panel
    // lists.
    use_effect(move || {
        let t = tree.read();
        let tab = active.read().clone();
        let drawn = match &tab {
            Some(Tab::Session(id)) if nav::find(&t, id).is_some() => {
                pictures::drawn(&gallery.read().store, id)
            }
            _ => Vec::new(),
        };
        app.dispatch(Msg::DetailsShown(Box::new(details::Told { drawn })));
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
        let _ = face();
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

    // The console's terminal fills what the prompt card leaves of the console,
    // in cells of the measured width; all of it when no card is shown.
    use_effect(move || {
        let (width, height) = console_box();
        let cw = cell_width();
        let _ = (face(), cell_height_now());
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
            let _ = (face(), tree.read(), attached());
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
                &settings.peek(),
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
    // Take what is written from the prompt to send. Taken now, so what is
    // typed while it is sent stays for the next prompt; it goes to the tab
    // it was written for, though another be shown by now.
    let take_prompt = {
        let hub = hub.clone();
        move || {
            let hub = hub.clone();
            spawn(async move {
                let (give, given) = oneshot::channel();
                let taken = match prompts.send(Op::Take(give)) {
                    Ok(()) => given.await.ok(),
                    Err(_) => None,
                };
                // Sent empty, the key itself goes to the session, as it would
                // in Claude Code's own input.
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
                    app.dispatch(Msg::PromptEmpty);
                    return;
                };
                let nv = nvim.peek().clone();
                if let Some(nv) = nv {
                    let _ = nv.stop_writing().await;
                }
                // Sent with nothing open, a prompt starts a scratch session,
                // as a draft there would.
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
                let agent = app.agent_of(Some(&tab));
                let taken = adroit_core::prompt::send::Taken {
                    tab: Some(tab),
                    text,
                    images,
                    agent,
                    ..Default::default()
                };
                app.dispatch(Msg::PromptTaken(Box::new(taken)));
            });
        }
    };

    // Send a prompt to the session it was written for, once it is ready for
    // input.
    let send_prompt = {
        let hub = hub.clone();
        move |taken: adroit_core::prompt::send::Taken| {
            let hub = hub.clone();
            spawn(async move {
                let size = *console_size.peek();
                let Some(tab) = taken.tab.clone() else {
                    app.dispatch(Msg::PromptWent(false));
                    return;
                };
                let sent = match pane_for(app, &hub, &tab, size).await {
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
                            app.status(Some("The session ended before it took the prompt".into()));
                            false
                        } else {
                            type_into(&p, &taken.typing).await;
                            true
                        }
                    }
                    None => {
                        app.status(Some("This session cannot be reached from here".into()));
                        false
                    }
                };
                app.dispatch(Msg::PromptWent(sent));
            });
        }
    };

    // Start a session from a draft with its first prompt.
    let start_draft = {
        let hub = hub.clone();
        move |asked: adroit_core::prompt::send::Start| {
            let hub = hub.clone();
            spawn(async move {
                let size = *console_size.peek();
                let adroit_core::prompt::send::Start {
                    draft,
                    name,
                    text,
                    options,
                    taken,
                } = asked;
                let Tab::Draft { worktree, .. } = &draft else {
                    app.dispatch(Msg::PromptWent(false));
                    return;
                };
                // Made only now, so a draft left unsent leaves no empty
                // directory behind.
                let started = match scratch::start_dir(worktree, &scratch_dir(), Moment::now()) {
                    Ok(worktree) => {
                        let request = crate::execution::StartRequest {
                            agent: taken.agent,
                            name: name.clone(),
                            cwd: worktree,
                            model: options.model,
                            permissions: options.permissions,
                            prompt: text,
                            images: taken.images.clone(),
                            env: daemon::agent_env(),
                            started_ms: now_ms(),
                            ..Default::default()
                        };
                        let settings = settings.peek().clone();
                        match crate::delegation::app::prepare(
                            &crate::delegation::app::root(),
                            &settings,
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
                        app.dispatch(Msg::PromptWent(true));
                    }
                    Err(e) => {
                        app.status(Some(e));
                        app.dispatch(Msg::PromptWent(false));
                    }
                }
            });
        }
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
                let settings = settings.peek().clone();
                let started = match crate::delegation::app::prepare(
                    &crate::delegation::app::root(),
                    &settings,
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

    // A new theme reaches every console open, what is drawn and what a
    // program asking for its colours is told.
    let theme = use_memo(move || settings.read().theme.clone());
    let themed = use_hook(|| Rc::new(RefCell::new(theme.peek().clone())));
    use_effect({
        let hub = hub.clone();
        move || {
            let theme = theme();
            if *themed.borrow() == theme {
                return;
            }
            themed.replace(theme);
            match palette_of(&settings.peek()) {
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
    });
    use_drop(settings_window::close);
    use_drop(image_window::close_all);

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

    // Type what the host asks into a session's pane, attaching to it if
    // need be.
    let type_into_tab = {
        let hub = hub.clone();
        move |tab: Tab, typing: Vec<Typing>| {
            let hub = hub.clone();
            spawn(async move {
                let size = *console_size.peek();
                if let Some(p) = pane_for(app, &hub, &tab, size).await {
                    type_into(&p, &typing).await;
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

    // The console read in a Neovim of its own, for Vim's moves, search,
    // selection and yank, while the console says it is read.
    let reading = use_reader(app, &hub, frame, console_size);
    let Reader {
        reader,
        reader_frame,
        reader_view,
        read_rows,
        ..
    } = reading.clone();
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
    let scroll_reader = use_callback({
        let reading = reading.clone();
        move |lines: i32| reading.scroll(lines)
    });
    let click_reader = use_callback({
        let reading = reading.clone();
        move |at: (usize, usize)| reading.click(at)
    });
    let consoles = Consoles {
        frame,
        attached,
        nvim,
        prompt_frame,
        prompt_view,
        prompt_rows,
        reader,
        reader_frame,
        reader_view,
        read_rows,
        cell_width,
        console_box,
        composer_height,
        prompt_width,
        prompts,
        scroll_reader,
        click_reader,
    };
    use_context_provider(|| consoles);

    // Claude Code reports no status for a dialog it opens itself, so the
    // console's screen tells, looked at again whenever it is redrawn.
    use_effect({
        let active_pane = active_pane.clone();
        move || {
            let _ = frame();
            let shows = match &*active.read() {
                Some(tab @ Tab::Session(_)) => {
                    let agent = app.agent_of(Some(tab));
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
        let hub = hub.clone();
        let panes = hub.panes.clone();
        let ask_input = ask_input.clone();
        let take_prompt = take_prompt.clone();
        let send_prompt = send_prompt.clone();
        let start_draft = start_draft.clone();
        let console_live = console_live.clone();
        let reading = reading.clone();
        let active_pane = active_pane.clone();
        let start_terminal = start_terminal.clone();
        let resume = resume.clone();
        let restart_daemon = restart_daemon.clone();
        let type_into_tab = type_into_tab.clone();
        move || {
            app.hand_over(move |effect| match effect {
                CoreEffect::Resume(past) => resume(past),
                CoreEffect::RestartDaemon => restart_daemon(),
                CoreEffect::TypeInto { tab, typing } => type_into_tab(tab, typing),
                CoreEffect::ShowShot { shot, session } => {
                    show_shot(&shot, &session, &gallery.peek())
                }
                CoreEffect::StartTerminal(ask) => start_terminal(ask),
                CoreEffect::PromptClose(to) => {
                    let _ = prompts.send(Op::Close(to));
                }
                CoreEffect::StartWriting => consoles.start_writing(),
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
                    reading.enter(keys.iter().filter_map(notation).collect());
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
                CoreEffect::TakePrompt => take_prompt(),
                CoreEffect::SendPrompt(taken) => send_prompt(*taken),
                CoreEffect::StartDraft(start) => start_draft(*start),
                CoreEffect::PromptShow(tab) => {
                    let _ = prompts.send(Op::Show(tab));
                }
                CoreEffect::PromptOffer(offers) => {
                    if let Some(nv) = nvim.peek().clone() {
                        let list = complete::offered(&offers);
                        spawn(async move {
                            let _ = nv.set_candidates(&list).await;
                        });
                    }
                }
                CoreEffect::PromptCommands(list) => {
                    if let Some(nv) = nvim.peek().clone() {
                        let list = complete::commands(&list);
                        spawn(async move {
                            let _ = nv.set_commands(&list).await;
                        });
                    }
                }
                CoreEffect::PromptRoot(root) => {
                    if let Some(nv) = nvim.peek().clone() {
                        spawn(async move {
                            let _ = nv.set_root(root.as_deref()).await;
                        });
                    }
                }
                CoreEffect::FilterFocus(on) => focus_filter(on),
                CoreEffect::PromptSent => {
                    let _ = prompts.send(Op::Sent);
                }
                CoreEffect::PromptRestore { tab, text, images } => {
                    let lines: Vec<String> = text.lines().map(String::from).collect();
                    let _ = prompts.send(Op::RestoreImages(tab.clone(), images));
                    let _ = prompts.send(Op::Add(tab, lines, None));
                }
                CoreEffect::RemoveImage(path) => {
                    let _ = prompts.send(Op::RemoveImage(path));
                }
                CoreEffect::PromptRecall { tab, back, sent } => {
                    let _ = prompts.send(Op::Recall(tab, back, sent));
                }
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
                CoreEffect::Viewer { window, action } => {
                    let mut viewer_command = viewer_command;
                    let seq = viewer_command.peek().as_ref().map_or(1, |(_, c)| c.seq + 1);
                    viewer_command.set(Some((window, viewers::Command { seq, action })));
                }
                CoreEffect::EndShell(id) => {
                    end_shell(&hub, id);
                    close_shell(id, &hub.panes, app);
                }
                CoreEffect::TakeSuggestion {
                    key,
                    neovim_owns,
                    session,
                    hands_back,
                    no_images,
                    tab,
                } => {
                    let Some(nv) = nvim.peek().clone() else {
                        return;
                    };
                    let pane = active_pane().filter(|_| session);
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
                _ => {}
            })
        }
    });

    // What the window knows of its programs now, which the view state routes
    // a key and runs an action by.
    let probe_now = {
        let active_pane = active_pane.clone();
        move || {
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
            let (inserting, prompt_owns) = nvim.peek().as_ref().map_or((false, false), |n| {
                let s = n.state();
                (
                    s.mode().starts_with("insert"),
                    s.popup().is_some() || s.mode().starts_with("cmdline"),
                )
            });
            Probe {
                pane: pane.is_some(),
                scrolls_itself: pane.as_ref().is_some_and(|p| p.takes_mouse()),
                reader,
                inserting,
                prompt_owns,
                rows: console_size.peek().rows,
            }
        }
    };

    // Run an action where the keys are now: what a binding runs, and what a
    // click on a row's or a tab's menu runs. `key` is the key that ran it,
    // which some actions pass on; a click has none.
    let perform = {
        let probe_now = probe_now.clone();
        move |action: Action, key: Option<Key>| {
            let probe = Box::new(probe_now());
            app.dispatch(Msg::Perform { action, key, probe });
        }
    };

    // What a menu entry runs, once the view state has selected what it was
    // opened on.
    use_hook({
        let perform = RefCell::new(perform.clone());
        move || {
            app.hand_perform(move |action, key| {
                if let Ok(perform) = perform.try_borrow_mut() {
                    perform(action, key);
                }
            })
        }
    });
    // Run an action from a button, where the keys are now.
    let run_button = {
        let perform = perform.clone();
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
    let as_alt = use_memo(move || settings.read().console.option_as_alt.name());
    use_effect(move || {
        let as_alt = as_alt();
        document::eval(&format!("window.adroitOptionAsAlt = '{as_alt}';"));
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
                app.dispatch(Msg::WindowFocused(*on));
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
        let perform = perform.clone();
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
        let probe_now = probe_now.clone();
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
            let probe = probe_now();
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
        let handle_key = handle_key.clone();
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
    let console_background = hub.palette.read().background.hex();
    // The theme's colours and the console's font, over the stylesheet's own.
    let root_style = {
        let s = settings.read();
        format!(
            ":root{{{theme}--term-font:{family};--term-size:{size}px;--term-weight:{weight};--cell-h:{h}px}}",
            theme = themes::css(&themes::window(&hub.palette.read())),
            family = s.console_font(),
            size = s.font_size,
            weight = s.font_weight,
            h = cell_height_now(),
        )
    };
    let (reading, passing, dialog) = {
        let console = app.console.read();
        (console.reading, console.direct, console.dialog)
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
        let jj = app
            .host
            .read()
            .work
            .as_ref()
            .and_then(|w| w.place.of(&p.worktree))
            .is_some_and(|r| r.jj);
        t.workspace_of(&p.worktree).is_some_and(|w| !w.git) && !jj
    });
    let active_tab = active();
    let current_agent = app.agent_of(active_tab.as_ref());
    let live_input_ms = active_tab.as_ref().and_then(|tab| {
        hub.panes
            .borrow()
            .get(tab)
            .and_then(|pane| pane.last_input_ms())
    });
    let (mode, selected_image) = {
        let prompt = app.prompt.read();
        (prompt.mode, prompt.selected.clone())
    };
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
            answering || (selected_image.is_none() && matches!(mode.0, "insert" | "cmd"))
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

    rsx! {
        style { "{root_style}" }
        div { class: "app",
            textarea { id: "adroit-keys", autofocus: true, readonly: !takes_text }
            settings_window::SettingsLoadError { error: app.config.read().settings_error.clone() }
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
                        onopen: EventHandler::new(move |item| app.dispatch(Msg::RowClicked(item))),
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
                            let run = run.clone();
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
                        usage_settings: settings.read().usage,
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
