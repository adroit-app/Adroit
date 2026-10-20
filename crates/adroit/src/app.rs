use std::cell::{Cell, RefCell};
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use adroit_claude::git::DiffKind;
use adroit_claude::links::{Kind as LinkKind, Link, Links, Touch};
use adroit_claude::usage::{self, Severity};
use adroit_claude::{commands, places};
use adroit_model::{InboxItem, Kind, Runs, Session, Shell, State, Tree, Worktree, build, inbox};
use adroit_nvim::{Embedded, Event as NvimEvent, notation};
use adroit_term::{Daemon, Key, Palette, Pane, Size, Spawn};
use dioxus::prelude::*;
use tokio::sync::{Notify, mpsc, oneshot};

use crate::agents;
use crate::alerts;
use crate::complete::{self, candidates};
use crate::completion::Completions;
use crate::console::{Composer, ConsoleScreen, Hub, Panes, ReaderScreen, mode_of};
use crate::console_keys::direct_key;
use crate::console_read;
use crate::daemon;
use crate::details::{
    Drill, Entry, Group, Listing, Related, Sheet, Shot, Source, Tie, View, capped, facts, gauges,
    load, overview_shown, related, shown_agent, shown_link, shown_shot, with_agents, with_images,
};
use crate::editor;
use crate::filer::{Filer, read_dirs};
use crate::focus::{Area, Dir, moved, settled};
use crate::graph;
use crate::icons::{Icon, IconView};
use crate::image_window;
use crate::keymap::{Action, Chord, Context, Keymap, MENU_KEYS, Step};
use crate::keys;
use crate::layout::{self, Layout, Side};
use crate::menu::{self, Target};
use crate::menu_bar;
use crate::nav::{self, Item, Place};
use crate::pasted;
use crate::paths::{home, instance_state, keys_path, scratch_dir};
use crate::picker::{self, Candidate, Choice, Mode, Picker};
use crate::picker_view::PickerView;
use crate::pictures::{self, Gallery};
use crate::programs;
use crate::prompts::{self, Op, Prompts};
use crate::rename::Rename;
use crate::scratch::{self, Moment};
use crate::sessions::{self, Kept};
use crate::settings;
use crate::settings_window;
use crate::shells::{Request, Response, answer, ask, bind, serve, socket_path};
use crate::status;
use crate::style::{CSS, CellHeight, DEFAULT_CELL_HEIGHT};
use crate::subagent_view::{SubagentView, agent_glyph};
use crate::tabs::{self, Role, Tab, fresh_name, label_from, named, shows_prompt};
use crate::themes;
use crate::view::Keys;
use crate::zoom;

fn shows_usage(tab: Option<&Tab>, agent: adroit_model::Agent) -> bool {
    matches!(tab, Some(Tab::Session(_) | Tab::Draft { .. }))
        && agents::adapter(agent).capabilities().usage_limits
}

/// The most directories the picker lists at once.
const PICKS: usize = 40;

/// The palette of the theme the settings name, or adroit's own when it
/// cannot be read.
fn initial_palette() -> Palette {
    themes::palette(
        &settings::current().theme,
        &settings::Settings::themes_dir(),
    )
    .unwrap_or_default()
}

/// The window's title: with what `git describe` said of the source, which
/// only a debug build is told.
fn title(described: Option<&str>) -> String {
    match described {
        Some(d) => format!("Adroit ({d})"),
        None => "Adroit".into(),
    }
}

/// Open the window.
pub fn launch() {
    use dioxus::desktop::{Config as Desktop, LogicalSize, WindowBuilder};
    settings::watch(Duration::from_secs(1));
    let cfg = Desktop::new()
        .with_window(
            WindowBuilder::new()
                .with_title(title(option_env!("ADROIT_DESCRIBE")))
                .with_inner_size(LogicalSize::new(1440.0, 900.0)),
        )
        .with_menu(menu_bar::build())
        .with_background_color((15, 17, 20, 255))
        .with_custom_head(format!("<style>{CSS}</style>"));
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
fn copy_to_clipboard(text: &str) -> std::io::Result<()> {
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

/// The overview's rows for the session `shown`, when its links were read for
/// it, the `folded` groups showing only their headings; the sessions it
/// worked with are found in `tree`, its images in `gallery`.
fn overview_listing(
    read: Option<&(String, Links)>,
    shown: Option<&Tab>,
    folded: &HashSet<Group>,
    opened: &HashSet<Group>,
    tree: &Tree,
    gallery: &Gallery,
) -> Option<Listing> {
    match (read, shown) {
        (Some((id, links)), Some(Tab::Session(s))) if id == s => {
            let rows: Vec<Link> = links.rows().cloned().collect();
            let sessions: Vec<&Session> = tree.sessions().map(|(_, s)| s).collect();
            let ties = match nav::find(tree, s) {
                Some((_, shown)) => related(shown, &sessions, &links.peers),
                None => Vec::new(),
            };
            let d = settings::current().details;
            let entries = overview_shown(
                &ties,
                &capped(&rows, d.links_kept),
                folded,
                opened,
                d.links_shown,
            );
            let mut shots = pictures::shots(&gallery.store, s, &links.images, &gallery.thumbs);
            shots.truncate(d.links_kept);
            let entries = with_agents(entries, &links.agents, folded);
            Some(Listing::Links(with_images(
                entries,
                &shots,
                folded,
                opened,
                d.links_shown,
            )))
        }
        _ => None,
    }
}

/// The session an empty prompt sent passes Ctrl+Enter to: the one whose tab
/// it was written for; a draft or a shell gets nothing from an empty one.
fn passes_enter(taken: Option<&(Option<Tab>, String)>) -> Option<String> {
    match taken {
        Some((Some(Tab::Session(id)), text)) if text.trim().is_empty() => Some(id.clone()),
        _ => None,
    }
}

/// `folded` with `group` folded, when `shut`, or unfolded.
fn refold(folded: &HashSet<Group>, group: Group, shut: bool) -> HashSet<Group> {
    let mut f = folded.clone();
    if shut {
        f.insert(group);
    } else {
        f.remove(&group);
    }
    f
}

/// `opened` once `group` is folded, when `shut`, or unfolded: a folded group
/// forgets it was shown whole, so it shows its first few when it opens again.
fn reopened(opened: &HashSet<Group>, group: Group, shut: bool) -> HashSet<Group> {
    let mut o = opened.clone();
    if shut {
        o.remove(&group);
    }
    o
}

/// A link as the overview draws it.
#[derive(Debug, Clone, PartialEq)]
struct LinkRow {
    /// Its place among the overview's rows.
    at: usize,
    icon: Icon,
    /// The class that colours its icon; empty for the panel's own colour.
    tone: &'static str,
    label: String,
    /// A chip after its label, as a subagent's kind.
    badge: Option<String>,
    tag: String,
    target: String,
    file: bool,
    /// A small copy of an image, drawn in place of the icon.
    thumb: Option<Arc<str>>,
}

/// A group of the overview's links as drawn: its heading, at its place among
/// the overview's rows, and the links shown under it.
#[derive(Debug, Clone, PartialEq)]
struct LinkSection {
    group: Group,
    at: usize,
    count: usize,
    folded: bool,
    rows: Vec<LinkRow>,
    /// The row for the links not shown: its place and how many they are.
    more: Option<(usize, usize)>,
}

/// The overview's `entries` as groups under their headings, paths shown
/// from `worktree` or `home`.
fn link_sections(entries: &[Entry], worktree: &Path, home: &Path) -> Vec<LinkSection> {
    let mut out: Vec<LinkSection> = Vec::new();
    for (at, entry) in entries.iter().enumerate() {
        match entry {
            Entry::More { hidden, .. } => {
                if let Some(section) = out.last_mut() {
                    section.more = Some((at, *hidden));
                }
            }
            Entry::Heading {
                group,
                count,
                folded,
            } => out.push(LinkSection {
                group: *group,
                at,
                count: *count,
                folded: *folded,
                rows: Vec::new(),
                more: None,
            }),
            Entry::Session(r) => {
                let icon = if r
                    .ties
                    .iter()
                    .any(|t| matches!(t, Tie::Started | Tie::StartedBy))
                {
                    Icon::MessagePlus
                } else {
                    Icon::Message
                };
                let mut tag: Vec<&str> = r.ties.iter().map(|t| t.word()).collect();
                if r.id.is_none() {
                    tag.push("not running");
                }
                if let Some(section) = out.last_mut() {
                    section.rows.push(LinkRow {
                        at,
                        icon,
                        tone: "",
                        label: r.name.clone(),
                        badge: None,
                        tag: tag.join(" · "),
                        target: r.name.clone(),
                        file: false,
                        thumb: None,
                    });
                }
            }
            Entry::Image(shot) => {
                let (label, tag) = shown_shot(shot, worktree, home);
                let (target, file) = match &shot.source {
                    Source::File { path, .. } => (path.clone(), true),
                    Source::Console { .. } => (label.clone(), false),
                };
                if let Some(section) = out.last_mut() {
                    section.rows.push(LinkRow {
                        at,
                        icon: Icon::Photo,
                        tone: "",
                        label,
                        badge: None,
                        tag,
                        target,
                        file,
                        thumb: shot.thumb.clone(),
                    });
                }
            }
            Entry::Agent(a) => {
                let (label, tag) = shown_agent(a, now_ms());
                if let Some(section) = out.last_mut() {
                    section.rows.push(LinkRow {
                        at,
                        icon: agent_glyph(a.standing).1,
                        tone: agent_glyph(a.standing).0,
                        label,
                        badge: a.kind.clone(),
                        tag,
                        target: a.call.clone(),
                        file: false,
                        thumb: None,
                    });
                }
            }
            Entry::Link(link) => {
                let icon = match link.kind {
                    LinkKind::Artifact => Icon::AppWindow,
                    LinkKind::Url => Icon::Link,
                    LinkKind::File(Touch::Edited) => Icon::FilePencil,
                    LinkKind::File(_) => Icon::File,
                };
                let (label, tag) = shown_link(link, worktree, home);
                if let Some(section) = out.last_mut() {
                    section.rows.push(LinkRow {
                        at,
                        icon,
                        tone: "",
                        label,
                        badge: None,
                        tag,
                        target: link.target.clone(),
                        file: matches!(link.kind, LinkKind::File(_)),
                        thumb: None,
                    });
                }
            }
        }
    }
    out
}

/// What is done with a link of the overview.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Use {
    Open,
    Copy,
    Reveal,
}

/// Open `link` in the app macOS opens it with, copy it, or show its file in
/// the Finder; what to tell of it.
fn use_link(link: &Link, how: Use) -> Option<String> {
    let open = |args: Vec<String>| {
        let mut cmd = tokio::process::Command::new("/usr/bin/open");
        cmd.args(args);
        spawn(async move {
            let _ = cmd.status().await;
        });
    };
    match how {
        Use::Open => {
            open(vec![link.target.clone()]);
            None
        }
        Use::Reveal if matches!(link.kind, LinkKind::File(_)) => {
            open(vec!["-R".into(), link.target.clone()]);
            None
        }
        Use::Reveal => None,
        Use::Copy => Some(match copy_to_clipboard(&link.target) {
            Ok(()) => format!("Copied {}", link.target),
            Err(e) => format!("cannot copy: {e}"),
        }),
    }
}

/// What using a session of the overview comes to.
#[derive(Debug, PartialEq, Eq)]
enum Went {
    /// Show the session by this id.
    Show(String),
    Told(String),
    Nowhere,
}

/// Show `related`, or copy its name, which Claude Code's messaging
/// addresses it by.
fn use_related(related: &Related, how: Use) -> Went {
    match (how, &related.id) {
        (Use::Open, Some(id)) => Went::Show(id.clone()),
        (Use::Open, None) => Went::Told(format!("{} is not running", related.name)),
        (Use::Copy, _) => Went::Told(match copy_to_clipboard(&related.name) {
            Ok(()) => format!("Copied {}", related.name),
            Err(e) => format!("cannot copy: {e}"),
        }),
        (Use::Reveal, _) => Went::Nowhere,
    }
}

/// Show `shot` in a window of its own, or copy or reveal its file as a file
/// of the overview is; what to tell of it. A drawn image is found among those
/// `gallery` keeps for `session`.
fn use_shot(shot: &Shot, how: Use, session: &str, gallery: &Gallery) -> Option<String> {
    match (&shot.source, how) {
        (
            Source::Console {
                id, width, height, ..
            },
            Use::Open,
        ) => {
            let kept = gallery.store.find(session, *id)?;
            image_window::open(
                &shot.key,
                &format!("Image {width} × {height}"),
                pictures::kept_shown(kept),
                Some((*width, *height)),
            );
            None
        }
        (Source::Console { .. }, Use::Copy) => Some("A drawn image has no file to copy".into()),
        (Source::Console { .. }, Use::Reveal) => None,
        (Source::File { path, .. }, Use::Open) => {
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
                    // Too large to read here: macOS's own viewer shows it.
                    None => {
                        let _ = tokio::process::Command::new("/usr/bin/open")
                            .arg(&path)
                            .status()
                            .await;
                    }
                }
            });
            None
        }
        (Source::File { path, touch }, how) => use_link(
            &Link {
                kind: LinkKind::File(*touch),
                target: path.clone(),
                title: None,
                line: None,
            },
            how,
        ),
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

/// How long Claude Code is given to call its external editor after `Ctrl-g`.
/// The answer clears it, so it only has to outlast a slow start: a request
/// arriving after it is taken for the user's own and opens their editor.
const PULL_WAIT: Duration = Duration::from_secs(10);

/// How long a notice stays in the status line. Errors and work under way stay
/// until something replaces them.
const NOTICE_FOR: Duration = Duration::from_secs(5);

/// The status line, and how many notices it has shown.
#[derive(Clone, Copy)]
struct Status {
    line: Signal<Option<String>>,
    notices: Signal<u64>,
}

impl Status {
    /// Show the error `text` while it lasts, or, with `None`, take back the
    /// one this source showed before; a message shown since stays.
    fn while_failing(mut self, last: &mut Option<String>, text: Option<String>) {
        let next = status::after_read(
            self.line.peek().as_deref(),
            last.as_deref(),
            text.as_deref(),
        );
        if let Some(line) = next {
            self.line.set(line);
        }
        *last = text;
    }

    /// Show `text` for a while: what just happened, or a hint for what was
    /// just tried, which would read as the state of things if it stayed.
    fn notice(mut self, text: String) {
        self.line.set(Some(text.clone()));
        let this = *self.notices.peek() + 1;
        self.notices.set(this);
        spawn(async move {
            tokio::time::sleep(NOTICE_FOR).await;
            if *self.notices.peek() == this && self.line.peek().as_deref() == Some(text.as_str()) {
                self.line.set(None);
            }
        });
    }
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

/// How long what git said about a worktree is trusted before asking again.
const WORKTREE_TTL: Duration = Duration::from_secs(15);

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
const INPUT: &str = r#"
const t = document.getElementById('adroit-keys');
// This run of the script. The window runs it again once the keys are lost,
// and this run then leaves the page to the later one.
const me = {};
window.adroitInput = me;
const current = () => window.adroitInput === me;
// While keys are traced, what reaches the window and where focus goes.
if (window.adroitDebugKeys) {
  const trace = (text) => current() && dioxus.send({ kind: 'trace', text });
  const name = (el) => !el ? 'none' : el.tagName.toLowerCase() + (el.id ? '#' + el.id : '') + (typeof el.className === 'string' && el.className ? '.' + el.className.trim().split(/\s+/).join('.') : '');
  window.addEventListener('keydown', (e) => trace('keydown key=' + e.key + ' code=' + e.code + ' keyCode=' + e.keyCode + ' composing=' + e.isComposing + ' meta=' + e.metaKey + ' target=' + name(e.target) + ' active=' + name(document.activeElement) + ' readOnly=' + t.readOnly + ' connected=' + t.isConnected + ' focus=' + document.hasFocus()), true);
  document.addEventListener('focusin', (e) => trace('focusin ' + name(e.target)), true);
  document.addEventListener('focusout', (e) => trace('focusout ' + name(e.target)), true);
  window.addEventListener('focus', () => trace('window focus'));
  window.addEventListener('blur', () => trace('window blur'));
  for (const k of ['compositionstart', 'compositionend']) t.addEventListener(k, (e) => trace(k + ' data=' + e.data));
  window.addEventListener('error', (e) => trace('error ' + e.message + ' at ' + e.lineno + ':' + e.colno));
  window.addEventListener('unhandledrejection', (e) => trace('rejection ' + e.reason));
}
// Whether the menu holds Command with this key: one of macOS's letters,
// whatever else is held, or a key an entry shows, written as the bindings
// write it. Shift is in a letter's case and in a character such as `}`, but
// not in a digit, which is read from its key.
const held = (e, name, fromCode) => {
  const keys = window.adroitHeld || [];
  if (keys.includes(name.toLowerCase())) return true;
  const character = [...name].length === 1 && name !== ' ';
  const shift = e.shiftKey && !/^[A-Z]$/.test(name) && !(character && !fromCode);
  const shown = name === ' ' ? 'Space' : name === '+' ? 'Plus' : name;
  return keys.includes('Cmd+' + (e.ctrlKey ? 'Ctrl+' : '') + (e.altKey ? 'Alt+' : '') + (shift ? 'Shift+' : '') + shown);
};
// The Option key held, `AltLeft` or `AltRight`, and whether the settings
// count it as Alt: `window.adroitOptionAsAlt` names which do.
let option = '';
t.addEventListener('keyup', (e) => {
  if (e.code === option) option = '';
});
const optionIsAlt = () => {
  const as = window.adroitOptionAsAlt || 'both';
  return as === 'both' || (as === 'left' && option === 'AltLeft') || (as === 'right' && option === 'AltRight');
};
// Whether the user types in this text field: one shown and taking text. A
// field hidden, gone or read-only would hold the keys without using them.
const typesIn = (a) => (a instanceof HTMLInputElement || a instanceof HTMLTextAreaElement)
  && a !== t && a.isConnected && !a.readOnly && !a.disabled && a.getClientRects().length > 0;
const take = () => {
  if (!current()) return;
  const a = document.activeElement;
  if (a !== t && !typesIn(a)) t.focus();
};
const keep = () => setTimeout(take, 0);
document.addEventListener('mouseup', keep);
window.addEventListener('focus', keep);
// Rust gives the keys back to the page when the window has them and the page
// has not.
const told = () => current() && dioxus.send({ kind: 'focused', on: document.hasFocus() });
window.addEventListener('focus', told);
window.addEventListener('blur', told);
told();
t.addEventListener('blur', keep);
// Whether the keys go to a terminal grid, where a wide character takes two
// cells rather than the width its font draws it in.
let onGrid = false;
let gridCursor = null;
// A key pressed on the text area, or `astray` on whatever else focus was on.
const onKey = (e, astray) => {
  if (!current()) return;
  // WebKit sends the key that ends a composition after `compositionend`,
  // no longer composing but still with key code 229.
  // Command with a key the menu does not hold is the window's own; the
  // letter is read from the key itself, as Command and Option change what it
  // types.
  if (e.metaKey && !e.isComposing && !['Meta', 'Shift', 'Control', 'Alt'].includes(e.key)) {
    const code = /^(Key[A-Z]|Digit[0-9])$/.exec(e.code);
    let name = code ? e.code.slice(-1) : e.key;
    if (code && !e.shiftKey) name = name.toLowerCase();
    if (held(e, name, !!code)) return;
    e.preventDefault();
    dioxus.send({ kind: 'key', key: name, ctrl: e.ctrlKey, alt: e.altKey, shift: e.shiftKey, meta: true });
    return;
  }
  if (e.isComposing || e.keyCode === 229 || e.metaKey) return;
  if (e.code === 'AltLeft' || e.code === 'AltRight') option = e.code;
  const printable = [...e.key].length === 1;
  // On a grid, an Option key that does not count as Alt types what macOS
  // makes of the key, as a terminal set so would.
  const alt = e.altKey && (!onGrid || optionIsAlt());
  if (printable && !e.ctrlKey && !alt) {
    // Read-only, the text area takes no text: the key is a command. Astray,
    // it never reached the text area to be typed.
    if (t.readOnly || astray) {
      e.preventDefault();
      dioxus.send({ kind: 'text', text: e.key });
    }
    return;
  }
  e.preventDefault();
  // Option turns a letter into another character on macOS (Option-h is
  // "˙"); the key itself is what a binding or a terminal's Meta means.
  let name = e.key;
  const base = e.altKey && /^(Key[A-Z]|Digit[0-9])$/.exec(e.code);
  if (base) {
    name = e.code.slice(-1);
    if (!e.shiftKey) name = name.toLowerCase();
  }
  dioxus.send({ kind: 'key', key: name, ctrl: e.ctrlKey, alt: e.altKey, shift: e.shiftKey });
};
t.addEventListener('keydown', (e) => onKey(e, false));
// A key that reaches the page while focus is where keys are not used, as on
// an element a click left focused, is the window's all the same. A dialog's
// keys are its own.
window.addEventListener('keydown', (e) => {
  const on = e.target;
  if (!current() || e.defaultPrevented || on === t || typesIn(on) || (on instanceof Element && on.closest('.dialog'))) return;
  t.focus();
  onKey(e, true);
});
// The text area sits on the cursor of what has the keys, so the input
// method draws its composition and opens its candidates there.
let at = '';
// The colour of the grid's cell under its cursor, which the text composed
// there is drawn in once it lands.
const colourAt = (cursor) => {
  const r = cursor.getBoundingClientRect();
  const x = r.left + 1, y = r.top + r.height / 2;
  for (const s of cursor.parentElement.querySelectorAll('.screen-line span')) {
    const b = s.getBoundingClientRect();
    if (x >= b.left && x < b.right && y >= b.top && y < b.bottom) return getComputedStyle(s).color;
  }
  return '';
};
const shown = document.getElementById('adroit-compose') || document.createElement('div');
shown.id = 'adroit-compose';
document.body.appendChild(shown);
const follow = () => {
  if (!current()) return;
  // The text area gone, as when the page is drawn anew, takes the listeners
  // with it: the window runs the script again.
  if (!t.isConnected) {
    dioxus.send({ kind: 'lost' });
    return;
  }
  // Focus left where the keys are not used, as on a text field gone or
  // hidden, gives them back to the text area.
  if (document.hasFocus()) take();
  const cursor = document.querySelector('.screen-cursor');
  const c = cursor
    || document.querySelector('.app-panel.focused .list-row.selected')
    || document.querySelector('.app-panel.focused');
  onGrid = !!cursor;
  gridCursor = cursor;
  if (c) {
    const r = c.getBoundingClientRect();
    const k = r.left + ',' + r.top;
    if (k !== at) {
      at = k;
      t.style.left = r.left + 'px';
      t.style.top = r.top + 'px';
      shown.style.left = r.left + 'px';
      shown.style.top = r.top + 'px';
    }
  }
  requestAnimationFrame(follow);
};
follow();
const probe = document.createElement('span');
probe.className = 'cell-probe';
document.body.appendChild(probe);
// East Asian wide and fullwidth characters, which a terminal gives two cells.
const wide = /[ᄀ-ᅟ⺀-〾ぁ-㏿㐀-䶿一-鿿ꀀ-꓏가-힣豈-﫿︰-﹏＀-｠￠-￦]|[\u{1F300}-\u{1F64F}\u{1F900}-\u{1F9FF}\u{20000}-\u{3FFFD}]/u;
let cell = 0;
t.addEventListener('compositionstart', () => {
  t.classList.add('composing');
  if (onGrid) {
    probe.textContent = 'M'.repeat(100);
    cell = probe.getBoundingClientRect().width / 100;
    shown.style.color = gridCursor ? colourAt(gridCursor) : '';
    t.classList.add('gridded');
    shown.classList.add('on');
  }
});
t.addEventListener('compositionupdate', (e) => {
  const text = e.data || '';
  probe.textContent = text;
  t.style.width = Math.ceil(probe.getBoundingClientRect().width) + 2 + 'px';
  if (!onGrid) return;
  // Each character is drawn in the cells the grid will give it once it is
  // committed, and as large as the grid draws it, so the text neither spreads
  // out nor grows when it lands.
  const height = parseFloat(getComputedStyle(document.documentElement).getPropertyValue('--cell-h'));
  shown.replaceChildren(...[...text].map((ch) => {
    const s = document.createElement('span');
    s.textContent = ch;
    const w = wide.test(ch);
    s.style.width = (w ? 2 : 1) * cell + 'px';
    if (w) s.style.fontSize = Math.min(2 * cell, height) + 'px';
    return s;
  }));
});
t.addEventListener('beforeinput', (e) => {
  if (e.isComposing || e.inputType === 'insertCompositionText') return;
  e.preventDefault();
  if (e.inputType === 'insertText' && e.data) dioxus.send({ kind: 'text', text: e.data });
});
t.addEventListener('paste', (e) => {
  e.preventDefault();
  const c = e.clipboardData;
  const d = c ? c.getData('text/plain') : '';
  if (d) {
    dioxus.send({ kind: 'paste', text: d });
    return;
  }
  // Only an image is read: Claude Code takes no video or other file.
  const f = c && [...c.items]
    .filter((i) => i.kind === 'file' && i.type.startsWith('image/'))
    .map((i) => i.getAsFile())
    .find((f) => f);
  if (!f) return;
  const r = new FileReader();
  r.onload = () => {
    const url = String(r.result);
    dioxus.send({ kind: 'image', type: f.type, data: url.slice(url.indexOf(',') + 1) });
  };
  r.readAsDataURL(f);
});
t.addEventListener('compositionend', (e) => {
  t.classList.remove('composing', 'gridded');
  shown.classList.remove('on');
  shown.replaceChildren();
  t.style.width = '';
  if (e.data) dioxus.send({ kind: 'text', text: e.data });
  setTimeout(() => { t.value = ''; }, 0);
});
keep();
"#;

/// The width and height of one cell, measured with the console's own font.
const MEASURE: &str = r#"
const s = document.createElement('span');
s.className = 'cell-probe';
s.textContent = 'M'.repeat(100);
document.body.appendChild(s);
const w = s.getBoundingClientRect().width / 100;
// A line as tall as the font asks for: its ascent, descent and gap, which is
// how a terminal sizes its cells.
s.style.lineHeight = 'normal';
s.style.display = 'inline-block';
const h = s.getBoundingClientRect().height;
s.remove();
dioxus.send([w, h]);
"#;

fn nvim_program() -> String {
    std::env::var("ADROIT_NVIM").unwrap_or_else(|_| "nvim".into())
}

/// This program, as a session should call it to reach the window.
fn adroit_program() -> PathBuf {
    std::env::current_exe().unwrap_or_else(|_| "adroit".into())
}

/// A question the window holds the keys for until it is answered.
#[derive(Debug, Clone, PartialEq)]
enum Ask {
    TakeOver(Session),
    Stop(Session),
}

/// Now, in milliseconds since the Unix epoch.
fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_millis() as u64)
}

/// Where the tab shown is kept for the next run of the window.
fn shown_path() -> PathBuf {
    instance_state().join("shown")
}

fn short(path: &Path) -> String {
    picker::short(path, &home())
}

fn diff_class(kind: DiffKind) -> &'static str {
    match kind {
        DiffKind::File => "diff-file",
        DiffKind::Hunk => "diff-hunk",
        DiffKind::Added => "diff-add",
        DiffKind::Removed => "diff-del",
        DiffKind::Context => "diff-ctx",
    }
}

/// Whether every key but the window's own goes to the program in the
/// console: a shell's from the console, and a session waiting on a choice
/// from the prompt as well, so its choices move without leaving the prompt.
fn passes(area: Area, session: bool, waits: bool) -> bool {
    match area {
        Area::Console => !session || waits,
        Area::Prompt => session && waits,
        Area::Sidebar | Area::Details => false,
    }
}

/// Where typed or pasted text goes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Typed {
    Prompt,
    /// The program in the console.
    Program,
    Nowhere,
}

/// Where text typed or pasted in `area` goes: text for a `session` is
/// written in the prompt, but while the console passes keys `direct`ly or
/// the session `waits` on an answer.
fn typed_into(area: Area, session: bool, direct: bool, waits: bool) -> Typed {
    match area {
        Area::Console if session && !direct && !waits => Typed::Prompt,
        Area::Console => Typed::Program,
        Area::Prompt if passes(area, session, waits) => Typed::Program,
        Area::Prompt => Typed::Prompt,
        Area::Sidebar | Area::Details => Typed::Nowhere,
    }
}

/// Whether a key that ends no binding starts reading the console. Read
/// already, the key broke off a `global` sequence, which is not the reader's:
/// `Ctrl+w s` would split its window.
fn starts_reading(area: Area, session: bool, passing: bool, reading: bool) -> bool {
    area == Area::Console && session && !passing && !reading
}

/// What the mode chip's word means, `dialog` while the session in the console
/// shows a dialog of Claude Code's own.
fn mode_tip(mode: &str, dialog: bool) -> &'static str {
    match mode {
        "PASS" if dialog => {
            "The session shows a dialog: every key goes to it, but Ctrl+w sequences, until it closes"
        }
        "TREE" => "The sidebar has the keys: j and k move, Enter opens. Ctrl+w l goes right",
        "DETAILS" => {
            "The details panel has the keys: j and k move or scroll, Enter opens, h goes back, Cmd+] and Cmd+[ change the sheet"
        }
        "CONSOLE" => {
            "The console has the keys: Vim's moves read it, i writes a prompt below, Ctrl+w h goes back to the sidebar"
        }
        "READ" | "VISUAL" => {
            "Vim's keys move, search, select and yank in the console; q or Esc goes back to it live, i writes a prompt"
        }
        "PASS" => "Every key goes to the console's program, but Ctrl+w sequences",
        "DIRECT" => "Every key goes to the session until Esc",
        "PICK" => "Choosing where a session works: type to narrow, Enter chooses, Esc closes",
        "INSERT" => "Writing a prompt: Ctrl+Enter sends it, Esc stops writing",
        "NORMAL" => "The prompt takes Vim commands: i writes again, Ctrl+w k goes up",
        _ => "The prompt is Neovim; this is its mode",
    }
}

fn row_badge(tree: &Tree, item: &Item) -> Option<&'static str> {
    match item {
        Item::Session(id) | Item::Waiting(id) => {
            nav::find(tree, id).map(|(_, s)| agents::adapter(s.agent).label())
        }
        _ => None,
    }
}

/// What a sidebar row is, told when the pointer rests on it.
fn row_tip(item: &Item) -> &'static str {
    match item {
        Item::Workspace(_) => {
            "A workspace: a git repository or a directory sessions run in. Enter folds it; n starts a session there"
        }
        Item::Worktree(_) => {
            "A worktree: a checkout of the repository, on a branch of its own. Enter or n starts a session in it, s a shell"
        }
        Item::Session(_) | Item::Waiting(_) => "A session. Enter opens it",
        Item::Shell(_) => "A shell adroit runs. Enter shows it; x closes it",
    }
}

/// A state's mark: its class, its icon, and the word told when the pointer
/// is over it.
fn glyph(state: &State) -> (&'static str, Icon, &'static str) {
    let (class, icon) = match state {
        State::Waiting(_) => ("state-glyph wait", Icon::Bell),
        State::Working => ("state-glyph run", Icon::Loader),
        State::Idle => ("state-glyph idle", Icon::Circle),
        State::Done => ("state-glyph done", Icon::Check),
        State::Failed => ("state-glyph fail", Icon::AlertTriangle),
    };
    (class, icon, state.word())
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

/// Close a shell's tab, and show another tab of its worktree if it was the
/// one on screen.
fn close_shell(
    id: u128,
    panes: &Panes,
    mut shells: Signal<Vec<Shell>>,
    mut active: Signal<Option<Tab>>,
    place: Signal<Option<Place>>,
    tree: Signal<Tree>,
    prompts: Prompts,
) {
    panes.borrow_mut().remove(&Tab::Shell(id));
    shells.write().retain(|s| s.id != id);
    if *active.peek() != Some(Tab::Shell(id)) {
        return;
    }
    let next = place.peek().as_ref().and_then(|p| {
        let left: Vec<u128> = shells
            .peek()
            .iter()
            .filter(|s| s.worktree == p.worktree)
            .map(|s| s.id)
            .collect();
        nav::tab_after_closing(&tree.peek(), p, &left)
    });
    let _ = prompts.send(Op::Close(next.clone()));
    active.set(next);
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
async fn take_input(
    path: &Path,
    pull: &mut Signal<Option<Tab>>,
    prompts: Prompts,
    notify: Status,
) -> Response {
    let mut status = notify.line;
    let Some(tab) = pull.peek().clone() else {
        return Response::Declined;
    };
    pull.set(None);
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
        status.set(Some(format!("cannot clear the session's input: {e}")));
        return Response::Taken(0);
    }
    let n = lines.len();
    notify.notice(moved_notice(n));
    Response::Taken(n)
}

fn moved_notice(n: usize) -> String {
    format!(
        "moved {n} line{} from the session's input to the prompt",
        if n == 1 { "" } else { "s" }
    )
}

/// The environment a session adroit starts runs in, over adroit's own.
fn session_env() -> Vec<(String, String)> {
    let mut env = daemon::terminal_env();
    // Claude Code runs the external editor with the environment it started
    // in, so the prompt can take back what was typed into its input.
    let editor = editor::script_path(&socket_path(&instance_state()));
    if ANSWERS.load(Ordering::Relaxed) && editor.exists() {
        // Kept for when the user opens the editor themselves.
        if let Ok(own) = std::env::var("VISUAL").or_else(|_| std::env::var("EDITOR")) {
            env.push((editor::USER_EDITOR.into(), own));
        }
        let editor = editor.display().to_string();
        env.push(("VISUAL".into(), editor.clone()));
        env.push(("EDITOR".into(), editor));
    }
    env
}

/// The daemon's id of the session a tab shows; `None` for a draft and for a
/// session running in another terminal.
fn kept_id(tab: &Tab) -> Option<u128> {
    match tab {
        Tab::Session(id) => sessions::id_of(id),
        Tab::Shell(id) => Some(*id),
        Tab::Draft { .. } => None,
    }
}

/// The pane a tab shows, attaching to its session when there is none yet:
/// `None` until the attach lands, when the tab is woken and `attached` told.
fn ensure_pane(
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
    });
    None
}

/// Start a session as `spawn` says, with the pane of the tab `tab_of` its id
/// names, so that its program starts in the theme's colours.
async fn start(
    hub: &Hub,
    spawn: Spawn,
    size: Size,
    tab_of: impl Fn(u128) -> Tab,
) -> adroit_term::Result<u128> {
    let palette = hub.palette.peek().clone();
    let wake = hub.wake.clone();
    let pane = Pane::start(&hub.daemon, spawn, size, &palette, |id| {
        let tab = tab_of(id);
        move || {
            let _ = wake.send(tab.clone());
        }
    })
    .await?;
    // A theme chosen while the session was being started passed it by.
    let now = hub.palette.peek().clone();
    if now != palette {
        pane.set_palette(&now);
    }
    let id = pane.id();
    hub.panes.borrow_mut().insert(tab_of(id), Arc::new(pane));
    Ok(id)
}

/// Every session to list: those the daemon runs for adroit, with the shells,
/// and those Claude Code lists in other terminals, but for adroit's own,
/// which Claude Code lists too.
async fn read_sessions(daemon: &Daemon) -> Result<(Vec<Session>, Vec<Shell>), String> {
    let kept = daemon.sessions().await.map_err(|e| e.to_string())?;
    let (mut own, mut kept_shells) = (Vec::new(), Vec::new());
    let mut learned = Vec::new();
    for s in &kept {
        match sessions::read(s) {
            Some(Kept::Agent(mut c)) => {
                let adapter = agents::adapter(c.agent);
                if let Some(title) = &s.title
                    && let Some(tag) = sessions::follow_title(&mut c, title)
                {
                    learned.push(tag);
                }
                if s.status.is_none()
                    && adapter.capabilities().screen_state
                    && let Ok(Ok(screen)) =
                        tokio::time::timeout(Duration::from_millis(300), daemon.visible(s.id)).await
                    && let Some(state) = adapter.screen_state(&screen)
                {
                    c.state = state;
                }
                own.push(c);
            }
            Some(Kept::Shell(sh)) => kept_shells.push(sh),
            None => {}
        }
    }
    // Without Claude Code, or with a listing it cannot give, the sessions in
    // other terminals are not listed; adroit's own still are.
    let mut elsewhere = Vec::new();
    for agent in adroit_model::Agent::ALL {
        let adapter = agents::adapter(agent);
        let listed = adapter.list_sessions().await.unwrap_or_default();
        let (external, tags) = adapter.follow(&mut own, listed);
        elsewhere.extend(external);
        learned.extend(tags);
    }
    // Kept on the session, so a window opened later follows it too; one not
    // kept is learned again on the next read.
    for (id, key, value) in learned {
        let held = kept
            .iter()
            .find(|s| s.id == id)
            .map(|s| adroit_term::tags::holding(&s.tags, key))
            .unwrap_or_default();
        let _ = daemon
            .tag(id, adroit_term::tags::tagged(key, &value), held)
            .await;
    }
    own.extend(elsewhere);
    Ok((own, kept_shells))
}

/// Whether the session `id` is still shown live here, or being attached to:
/// one started a moment ago may not be listed yet.
fn alive_here(panes: &Panes, attaching: &RefCell<HashSet<Tab>>, id: &str) -> bool {
    let tab = Tab::Session(id.to_string());
    attaching.borrow().contains(&tab) || panes.borrow().get(&tab).is_some_and(|p| !p.exited())
}

/// The pane a tab shows, waiting a while for its attach when there is none.
async fn pane_for(hub: &Hub, tab: &Tab, size: Size) -> Option<Arc<Pane>> {
    for _ in 0..ATTACH_WAIT_STEPS {
        if let Some(p) = ensure_pane(hub, tab, size, || {}) {
            return Some(p);
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    None
}

/// The action that shows `sheet`.
fn sheet_action(sheet: Sheet) -> Action {
    match sheet {
        Sheet::Overview => Action::DetailsOverview,
        Sheet::Changes => Action::DetailsChanges,
        Sheet::Log => Action::DetailsLog,
    }
}

/// The mark of a session that only reads.
#[component]
fn Book() -> Element {
    rsx! {
        span { class: "reads-only-mark", title: "only reads",
            IconView { icon: Icon::Book, size: 13 }
        }
    }
}

/// What the shell `id` runs; a shell when it is not among `shells`.
fn runs_of(shells: &[Shell], id: u128) -> Runs {
    shells
        .iter()
        .find(|s| s.id == id)
        .map(|s| s.runs)
        .unwrap_or_default()
}

/// The mark of a shell's row or tab, by what it runs.
#[component]
fn Terminal(runs: Runs) -> Element {
    let (icon, title) = match runs {
        Runs::Shell => (Icon::Terminal2, "shell"),
        Runs::Neovim => (Icon::Code, "Neovim"),
    };
    rsx! {
        span { class: "shell-mark", title,
            IconView { icon }
        }
    }
}

#[component]
fn App() -> Element {
    let mut tree = use_signal(Tree::default);
    let mut selected = use_signal(|| None::<Item>);
    let mut collapsed = use_signal(HashSet::<String>::new);
    let mut focus = use_signal(|| Area::Sidebar);
    // Whether every worktree git knows is listed, those without sessions too.
    let mut all_worktrees = use_signal(|| false);
    // The picker going to a session or starting one, while it is open, and
    // the directories it offers.
    let mut picking = use_signal(|| None::<Picker>);
    let mut offers = use_signal(Vec::<Candidate>::new);
    // What the open picker draws, worked out again only when the picker or
    // its offers change: the window redraws on every frame of the console,
    // and the tree reads the disk.
    // Moving the cursor changes neither what was typed nor what the tree has
    // open, so neither the ranking nor the disk is gone over again for it.
    let picked_query = use_memo(move || picking.read().as_ref().map(|p| p.query.clone()));
    let picked_layout = use_memo(move || {
        picking
            .read()
            .as_ref()
            .filter(|p| p.mode == Mode::Tree)
            .map(|p| p.filer.layout())
    });
    // The sessions are offered as they stand now, their states kept current.
    let picked_rows = use_memo(move || {
        picked_query
            .read()
            .as_ref()
            .map(|q| {
                let sessions = picker::sessions(&tree.read());
                picker::choices(&sessions, &offers.read(), q, &home(), PICKS)
            })
            .unwrap_or_default()
    });
    let picked_tree = use_memo(move || {
        picked_layout
            .read()
            .as_ref()
            .map(|f| f.rows(read_dirs))
            .unwrap_or_default()
    });
    let mut active = use_signal(|| None::<Tab>);
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
    // The agent chosen for a session not started yet, by its draft; the
    // default for new sessions otherwise.
    let mut draft_agents = use_signal(HashMap::<Tab, adroit_model::Agent>::new);
    let mut last_agent = use_signal(|| None::<adroit_model::Agent>);
    let mut place = use_signal(|| None::<Place>);
    let mut shells = use_signal(Vec::<Shell>::new);
    // The sessions opened in the window, which the tabs above the console
    // list; the sidebar lists every one.
    let mut opened = use_signal(HashSet::<Tab>::new);
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
    // selection and yank: whether the console's keys go to it, and it.
    let mut reading = use_signal(|| false);
    let mut reader = use_signal(|| None::<Arc<Embedded>>);
    let mut reader_frame = use_signal(|| 0u64);
    // The key the reader took last, for the second key of `gg`.
    let mut read_last = use_signal(|| None::<adroit_term::Key>);
    // The line and column the reader shows from, which images follow.
    let mut reader_view = use_signal(|| (0usize, 0usize));
    // How many lines the reader holds, and how many of them are the screen,
    // the last.
    let mut read_rows = use_signal(|| (0usize, 0usize));
    let mut zoomed = use_signal(|| settings::current().zoom);
    // The settings the console's font is drawn by, and the height of a line
    // measured in it.
    let mut font = use_signal(settings::current);
    let mut cell_height_now = use_signal(|| DEFAULT_CELL_HEIGHT);
    use_context_provider(|| CellHeight(cell_height_now));
    // The theme's palette, which a new theme replaces.
    let palette = use_signal(initial_palette);
    // The side panels' widths and whether they are shown, kept across runs.
    let mut layout = use_signal(Layout::load);
    // A side panel's edge being dragged: which, and where the pointer and
    // the width were when it was taken.
    let mut dragging = use_signal(|| None::<(Side, f64, f64)>);
    let mut body_width = use_signal(|| 1200.0_f64);
    // Hiding the panel the keys are in gives them to the console.
    let mut toggle_panel = move |side: Side| {
        let next = layout.peek().toggled(side);
        layout.set(next);
        next.save();
        let hidden = match side {
            Side::Sidebar => !next.show_sidebar && *focus.peek() == Area::Sidebar,
            Side::Details => !next.show_details && *focus.peek() == Area::Details,
        };
        if hidden {
            focus.set(Area::Console);
        }
    };
    // The tabs with something written for them kept aside.
    let drafted = use_signal(HashSet::<Tab>::new);
    let prompts = use_hook(|| prompts::keeper(nvim, drafted));
    let mut status = use_signal(|| None::<String>);
    let notices = use_signal(|| 0u64);
    let notify = Status {
        line: status,
        notices,
    };
    // The keys pressed so far of a binding not complete yet.
    let mut pending = use_signal(Vec::<Key>::new);
    let mut keymap = use_signal(Keymap::defaults);
    // Whether the next key goes to the session as it is.
    let mut send_next = use_signal(|| false);
    // How far into which sheet the details panel has gone, and what was read
    // for it.
    let mut drill = use_signal(|| Drill::new(Sheet::Overview));
    let spot = use_memo(move || drill().spot());
    let mut git_view = use_signal(|| None::<View>);
    // What the conversation of a session brought up, by the session's id.
    let mut overview_links = use_signal(|| None::<(String, Links)>);
    // The images the consoles drew, and the small copies of image files.
    let mut gallery = use_signal(Gallery::default);
    // The overview's groups of links shown by their headings alone.
    let mut folded_groups = use_signal(HashSet::<Group>::new);
    // The overview's groups of links shown whole, past their first few.
    let mut opened_groups = use_signal(HashSet::<Group>::new);
    // The interactive session the dialog asks to take over.
    let mut confirm = use_signal(|| None::<Ask>);
    // The session being given a new name, and the name typed so far.
    let mut renaming = use_signal(|| None::<Rename>);
    // The menu a right click opened, where it was clicked and on what.
    let mut menu = use_signal(|| None::<(f64, f64, Target)>);
    // Whether the console passes every key on to the session it shows.
    let mut direct = use_signal(|| false);
    // Whether the session in the console shows a dialog of Claude Code's own,
    // which, as a choice it waits on, takes every key.
    let mut dialog = use_signal(|| false);
    // The session asked to hand its input over through the external editor.
    let mut pull = use_signal(|| None::<Tab>);
    // Counts the requests, so one lapsing does not take a later one with it.
    let mut pulls = use_signal(|| 0u64);
    // The sessions whose input may hold text since it was last taken: keys
    // went to them as they are, or they were just attached.
    let mut stale = use_signal(HashSet::<Tab>::new);
    // The sessions being taken over: stopped, and not yet resumed here.
    let mut moving = use_signal(HashSet::<String>::new);
    // Whether the tab kept from the last run had its chance to come back;
    // until then, the empty window is not kept over it.
    let mut restored = use_signal(|| false);
    use_effect(move || {
        let shown = active.read().clone();
        if !restored() {
            return;
        }
        match shown {
            Some(tab) => {
                if let Some(name) = tabs::kept_name(&tab) {
                    let _ = std::fs::write(shown_path(), name);
                }
            }
            None => {
                let _ = std::fs::remove_file(shown_path());
            }
        }
    });
    // Keys meant for one area do not carry over to another: a sequence begun
    // in the sidebar would otherwise complete as a console binding after a
    // click there.
    use_effect(move || {
        let (area, _) = (focus(), active());
        if area != Area::Console && *direct.peek() {
            direct.set(false);
        }
        if !pending.peek().is_empty() {
            pending.set(Vec::new());
        }
        if *send_next.peek() {
            send_next.set(false);
        }
    });

    let hub = use_hook(|| {
        let (tx, mut rx) = mpsc::unbounded_channel::<Tab>();
        let panes: Panes = Rc::new(RefCell::new(HashMap::new()));
        // Coalesce wakeups into at most one redraw a frame, and only for the
        // pane on screen; another pane is drawn afresh when it is shown. A
        // shell that has exited has its tab closed.
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
                    if let (Tab::Shell(id), true) = (tab, gone) {
                        close_shell(*id, &watched, shells, active, place, tree, prompts);
                    }
                }
                let shown = active.peek().clone();
                if shown.is_some_and(|s| woke.contains(&s)) {
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
            refresh: Rc::new(Notify::new()),
            git: Rc::new(Notify::new()),
        }
    });
    use_context_provider({
        let hub = hub.clone();
        move || hub
    });

    // Sessions from the daemon and from Claude Code, placed by what git says
    // of where they run.
    use_hook({
        let refresh = hub.refresh.clone();
        let panes = hub.panes.clone();
        let attaching = hub.attaching.clone();
        let daemon = hub.daemon.clone();
        move || {
            spawn(async move {
                let compiled = tokio::task::spawn_blocking(|| {
                    daemon::install_terminfo(&daemon::terminfo_dir())
                })
                .await;
                if let Ok(Err(e)) = compiled {
                    status.set(Some(format!(
                        "cannot install the terminal's description: {e}"
                    )));
                }
                // The daemon may not run yet: started as adroit's own.
                if let Err(e) = daemon.start(&adroit_program(), &daemon::serve_args()).await {
                    status.set(Some(format!("cannot start the sessions' daemon: {e}")));
                }
                // What git said of each directory a session runs in, and of
                // each repository's worktrees, with when it said it.
                let mut cache: HashMap<PathBuf, (Instant, Option<Worktree>)> = HashMap::new();
                let mut lists: HashMap<PathBuf, (Instant, Vec<Worktree>)> = HashMap::new();
                let mut keys_read = None::<Option<String>>;
                let mut restore = std::fs::read_to_string(shown_path())
                    .ok()
                    .and_then(|name| tabs::from_kept(&name));
                // What each source last failed with, to take back once it reads.
                let (mut keys_err, mut list_err) = (None, None);
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
                                keymap.set(map);
                                None
                            }
                            Err(e) => Some(format!("{}: {e}", short(&keys_path()))),
                        };
                        notify.while_failing(&mut keys_err, failed);
                        keys_read = Some(keys_text);
                    }
                    match read_sessions(&daemon).await {
                        Ok((sessions, kept_shells)) => {
                            notify.while_failing(&mut list_err, None);
                            if *shells.peek() != kept_shells {
                                shells.set(kept_shells.clone());
                            }
                            // Asked of git all at once: one by one, every
                            // directory would hold the list back by its calls.
                            {
                                let mut unread: Vec<PathBuf> = sessions
                                    .iter()
                                    .map(|s| s.cwd.clone())
                                    .chain(kept_shells.iter().map(|s| s.worktree.clone()))
                                    .filter(|d| {
                                        cache
                                            .get(d)
                                            .is_none_or(|(at, _)| at.elapsed() > WORKTREE_TTL)
                                    })
                                    .collect();
                                unread.sort();
                                unread.dedup();
                                let mut asking = tokio::task::JoinSet::new();
                                for dir in unread {
                                    asking.spawn(async move {
                                        let w = adroit_claude::inspect(&dir).await;
                                        (dir, w)
                                    });
                                }
                                while let Some(answered) = asking.join_next().await {
                                    if let Ok((dir, w)) = answered {
                                        cache.insert(dir, (Instant::now(), w));
                                    }
                                }
                            }
                            let found: HashMap<PathBuf, Option<Worktree>> = sessions
                                .iter()
                                .map(|s| &s.cwd)
                                .chain(kept_shells.iter().map(|s| &s.worktree))
                                .filter_map(|d| cache.get(d).map(|(_, w)| (d.clone(), w.clone())))
                                .collect();
                            let mut listed = Vec::new();
                            if *all_worktrees.peek() {
                                let mut repos: Vec<PathBuf> =
                                    found.values().flatten().map(|w| w.repo.clone()).collect();
                                repos.sort();
                                repos.dedup();
                                // Asked all at once, as the directories are.
                                let mut asking = tokio::task::JoinSet::new();
                                for repo in repos.iter().filter(|r| {
                                    lists
                                        .get(*r)
                                        .is_none_or(|(at, _)| at.elapsed() > WORKTREE_TTL)
                                }) {
                                    let repo = repo.clone();
                                    asking.spawn(async move {
                                        let ws = adroit_claude::git::worktrees(&repo)
                                            .await
                                            .unwrap_or_default();
                                        (repo, ws)
                                    });
                                }
                                while let Some(answered) = asking.join_next().await {
                                    if let Ok((repo, ws)) = answered {
                                        lists.insert(repo, (Instant::now(), ws));
                                    }
                                }
                                for repo in repos {
                                    if let Some((_, ws)) = lists.get(&repo) {
                                        listed.extend(ws.iter().cloned());
                                    }
                                }
                            }
                            let scratch = scratch_dir();
                            let next =
                                build(&sessions, &kept_shells, &found, &listed, Some(&scratch));
                            // A session started a moment ago, and shown, is
                            // placed among its worktree's tabs once listed:
                            // below the worktree's top, it is shown there.
                            if let Some(Tab::Session(id)) = active.peek().clone()
                                && place.peek().is_some()
                                && let Some((p, _)) = nav::find(&next, &id)
                                && place.peek().as_ref() != Some(&p)
                                && nav::find(&tree.peek(), &id).is_none()
                            {
                                place.set(Some(p));
                            }
                            // The tab and the selected row each follow their
                            // session, which may be taken over from the sidebar
                            // while another tab is shown.
                            let _ = prompts.send(Op::Follow(sessions.clone()));
                            let shown = active.peek().clone();
                            if let Some(Tab::Session(old)) = shown
                                && let Some(new) = nav::successor(&sessions, &old)
                            {
                                active.set(Some(Tab::Session(new.to_string())));
                            }
                            // A session gone from the list, not merely gone by
                            // another id, gives its tab up as a closed shell
                            // does. One being taken over is only between ids.
                            let shown = active.peek().clone();
                            if let Some(Tab::Session(id)) = &shown
                                && nav::is_gone(&sessions, id)
                                && !moving.peek().contains(id)
                                && !alive_here(&panes, &attaching, id)
                            {
                                let after = place.peek().as_ref().and_then(|p| {
                                    let left: Vec<u128> = shells
                                        .peek()
                                        .iter()
                                        .filter(|s| s.worktree == p.worktree)
                                        .map(|s| s.id)
                                        .collect();
                                    nav::tab_after_closing(&next, p, &left)
                                });
                                // Said, or the tab would vanish unexplained.
                                if let Some((_, s)) = nav::find(&tree.peek(), id) {
                                    notify.notice(format!("{} ended", s.name));
                                }
                                let _ = prompts.send(Op::Close(after.clone()));
                                active.set(after);
                            }
                            let moved = selected
                                .peek()
                                .as_ref()
                                .and_then(|item| nav::successor_row(&sessions, item));
                            if let Some(item) = moved {
                                selected.set(Some(item));
                            }
                            // So does what is waiting to be taken from a
                            // session's input.
                            let still = nav::followed(&sessions, &opened.peek());
                            if *opened.peek() != still {
                                opened.set(still);
                            }
                            let kept = nav::followed(&sessions, &stale.peek());
                            if *stale.peek() != kept {
                                stale.set(kept);
                            }
                            let asked = pull.peek().clone();
                            if let Some(Tab::Session(old)) = asked
                                && let Some(new) = nav::successor(&sessions, &old)
                            {
                                pull.set(Some(Tab::Session(new.to_string())));
                            }
                            // A session no longer listed has nothing to show;
                            // its pane is closed.
                            panes.borrow_mut().retain(|tab, _| match tab {
                                Tab::Session(id) => sessions.iter().any(|s| &s.id == id),
                                Tab::Shell(id) => shells.peek().iter().any(|s| s.id == *id),
                                Tab::Draft { .. } => true,
                            });
                            // Nor are the images its console drew kept.
                            let gone = gallery
                                .peek()
                                .store
                                .sessions()
                                .any(|id| !sessions.iter().any(|s| s.id == id));
                            if gone {
                                gallery
                                    .write()
                                    .store
                                    .retain(|id| sessions.iter().any(|s| s.id == id));
                            }
                            // The tab shown when the window last closed comes
                            // back once what it shows is listed, unless
                            // another was opened meanwhile.
                            if let Some(kept) = restore.take()
                                && active.peek().is_none()
                            {
                                let at = match &kept {
                                    Tab::Session(id) => nav::find(&next, id).map(|(p, _)| p),
                                    Tab::Shell(id) => {
                                        shells.peek().iter().find(|s| s.id == *id).map(|s| Place {
                                            worktree: s.worktree.clone(),
                                        })
                                    }
                                    Tab::Draft { .. } => None,
                                };
                                if let Some(p) = at {
                                    if let Tab::Session(id) = &kept {
                                        selected.set(Some(Item::Session(id.clone())));
                                    }
                                    place.set(Some(p));
                                    active.set(Some(kept));
                                }
                            }
                            // Told only of what changed since the window first read the
                            // sessions, and only while it is not in front.
                            if *restored.peek()
                                && settings::current().notify_waiting
                                && !dioxus::desktop::window().is_focused()
                            {
                                for (name, why) in alerts::newly_waiting(&tree.peek(), &next) {
                                    alerts::notify_waiting(&name, &why);
                                }
                            }
                            if !*restored.peek() {
                                restored.set(true);
                            }
                            if *tree.peek() != next {
                                tree.set(next);
                            }
                        }
                        Err(e) => notify.while_failing(&mut list_err, Some(e.to_string())),
                    }
                    tokio::select! {
                        _ = tokio::time::sleep(Duration::from_secs(2)) => {}
                        // Asked for by hand: look at every worktree again too.
                        _ = refresh.notified() => {
                            cache.clear();
                            lists.clear();
                        }
                    }
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
                Some(p) => adroit_claude::git::files(&p.worktree)
                    .await
                    .unwrap_or_default(),
                None => Vec::new(),
            };
            // A listing that took longer than the next one is for a worktree
            // no longer on screen.
            if *place.peek() == p {
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
        let agent = agents::for_tab(
            &t,
            tab.as_ref(),
            &draft_agents.read(),
            last_agent.read().unwrap_or(font.read().agent),
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
            let got = agents::adapter(agent).commands(dir.clone()).await;
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

    // What git or jj says about the worktree in the middle, for the details
    // panel: read only while a sheet that reads it is shown, since every jj
    // command may record the working copy.
    use_hook({
        let wake = hub.git.clone();
        move || {
            spawn(async move {
                loop {
                    let wt = place.peek().as_ref().map(|p| p.worktree.clone());
                    let spot = drill.peek().spot();
                    let next = match (spot, wt) {
                        (Some(s), Some(wt)) => Some(load(s, wt).await),
                        _ => None,
                    };
                    // The panel may have moved on, or to another worktree,
                    // while it was read.
                    let mut stepped_back = false;
                    if let Some(v) = &next
                        && drill.peek().spot().as_ref() == Some(&v.spot)
                        && place.peek().as_ref().map(|p| &p.worktree) == Some(&v.worktree)
                    {
                        let mut d = drill.peek().clone();
                        stepped_back = d.refreshed(&v.listing);
                        if d != *drill.peek() {
                            drill.set(d);
                        }
                    }
                    if *git_view.peek() != next {
                        git_view.set(next);
                    }
                    if stepped_back {
                        continue;
                    }
                    tokio::select! {
                        _ = tokio::time::sleep(Duration::from_secs(2)) => {}
                        _ = wake.notified() => {}
                    }
                }
            });
        }
    });
    use_effect({
        let wake = hub.git.clone();
        move || {
            let _ = (spot(), place());
            wake.notify_one();
        }
    });
    let mut system_snapshot = use_signal(crate::system_usage::Snapshot::default);
    use_future(move || async move {
        let mut sampler = crate::system_usage::Sampler::default();
        loop {
            let settings = font.peek().usage;
            if layout.peek().show_details && settings.any() {
                let result = tokio::task::spawn_blocking(move || {
                    let snapshot = sampler.sample(settings);
                    (sampler, snapshot)
                })
                .await;
                match result {
                    Ok((next, mut snapshot)) => {
                        sampler = next;
                        if settings.gpu {
                            snapshot.gpu = crate::system_usage::gpu_usage().await;
                        }
                        system_snapshot.set(snapshot);
                    }
                    Err(_) => sampler = crate::system_usage::Sampler::default(),
                }
            }
            tokio::time::sleep(Duration::from_secs(2)).await;
        }
    });
    // How much of the account's usage limits is left, for the foot of the
    // details panel: asked of the console's agent while details are in sight, again
    // once the answer has aged.
    let mut usage_limits = use_signal(HashMap::<adroit_model::Agent, usage::Fresh>::new);
    let usage_wake = use_hook(|| Arc::new(Notify::new()));
    use_hook({
        let wake = usage_wake.clone();
        move || {
            spawn(async move {
                loop {
                    let agent = agents::for_tab(
                        &tree.peek(),
                        active.peek().as_ref(),
                        &draft_agents.peek(),
                        last_agent.peek().unwrap_or(settings::current().agent),
                    );
                    let in_sight =
                        layout.peek().show_details && shows_usage(active.peek().as_ref(), agent);
                    if in_sight
                        && usage_limits
                            .write()
                            .entry(agent)
                            .or_default()
                            .want(Instant::now())
                    {
                        let got = agents::adapter(agent).usage(home()).await;
                        usage_limits
                            .write()
                            .entry(agent)
                            .or_default()
                            .settle(Instant::now(), got);
                    }
                    tokio::select! {
                        _ = tokio::time::sleep(Duration::from_secs(30)) => {}
                        _ = wake.notified() => {}
                    }
                }
            });
        }
    });
    use_effect({
        let wake = usage_wake.clone();
        move || {
            let _ = layout().show_details;
            let _ = agents::for_tab(
                &tree.read(),
                active.read().as_ref(),
                &draft_agents.read(),
                last_agent.read().unwrap_or(font.read().agent),
            );
            wake.notify_one();
        }
    });
    // What the shown session's conversation brought up, for the overview:
    // read while the overview shows it, each time only what its transcript
    // gained since.
    let links_wake = use_hook(|| Arc::new(Notify::new()));
    use_hook({
        let wake = links_wake.clone();
        move || {
            spawn(async move {
                type ConversationKey = (adroit_model::Agent, PathBuf, String);
                type Reader = Box<dyn agents::ConversationReader>;
                let mut held: Option<(ConversationKey, Reader)> = None;
                loop {
                    let shown = match (drill.peek().sheet(), &*active.peek()) {
                        (Sheet::Overview, Some(Tab::Session(id))) => {
                            nav::find(&tree.peek(), id).map(|(_, s)| s.clone())
                        }
                        _ => None,
                    }
                    .filter(|s| {
                        agents::adapter(s.agent).capabilities().transcript
                            && !s.session_id.is_empty()
                    });
                    if let Some(s) = shown {
                        let key = (s.agent, s.cwd.clone(), s.session_id.clone());
                        let reader = match held.take() {
                            Some((was, reader)) if was == key => Some(reader),
                            _ => agents::adapter(s.agent).transcript(&home(), &s),
                        };
                        if let Some(t) = reader {
                            let thumbs = gallery.peek().thumbs.clone();
                            let read = tokio::task::spawn_blocking(move || {
                                let mut t = t;
                                let mut links = t.read();
                                let mut thumbs = thumbs;
                                pictures::refresh_thumbs(&mut links.images, &mut thumbs);
                                (t, links, thumbs)
                            })
                            .await;
                            if let Ok((t, links, thumbs)) = read {
                                held = Some((key, t));
                                if gallery.peek().thumbs != thumbs {
                                    gallery.write().thumbs = thumbs;
                                }
                                let next = Some((s.id, links));
                                if *overview_links.peek() != next {
                                    overview_links.set(next);
                                }
                            }
                        }
                    } else {
                        held = None;
                    }
                    tokio::select! {
                        _ = tokio::time::sleep(Duration::from_secs(2)) => {}
                        _ = wake.notified() => {}
                    }
                }
            });
        }
    });
    use_effect({
        let wake = links_wake.clone();
        move || {
            let _ = (drill().sheet(), active());
            wake.notify_one();
        }
    });
    // A subagent opened that the overview no longer lists is gone back from.
    use_effect(move || {
        let mut d = drill();
        let gone = d.agent().is_some_and(|call| {
            let read = overview_links();
            !read
                .as_ref()
                .is_some_and(|(_, l)| l.agents.iter().any(|a| a.call == call))
        });
        if gone && d.back() {
            drill.set(d);
        }
    });
    // What was opened in one worktree means nothing in another.
    let shown_worktree = use_memo(move || place().map(|p| p.worktree));
    use_effect(move || {
        let _ = shown_worktree();
        let top = Drill::new(drill.peek().sheet());
        if *drill.peek() != top {
            drill.set(top);
        }
    });
    // Keep the row selected in the details panel in view, after it is drawn.
    use_effect(move || {
        let _ = (drill(), git_view());
        document::eval(
            "requestAnimationFrame(() => document.querySelectorAll('.details-scroll .selected')
               .forEach((r) => r.scrollIntoView({ block: 'nearest' })));",
        );
    });

    // Answer sessions asking for the shells of their worktree, and Claude
    // Code's external editor handing over its input.
    use_hook({
        let hub = hub.clone();
        move || {
            let socket = socket_path(&instance_state());
            spawn(async move {
                let mut asked = match bind(&socket) {
                    Ok(listener) => {
                        // Only the window that answers on the socket can take
                        // Claude Code's input; another one would hand it to
                        // this one, which never asked.
                        match editor::install(&adroit_program(), &socket) {
                            Ok(_) => ANSWERS.store(true, Ordering::Relaxed),
                            Err(e) => {
                                status.set(Some(format!("cannot write the editor script: {e}")))
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
                        status.set(Some(format!("sessions cannot read the shells: {e}")));
                        return;
                    }
                };
                while let Some((request, reply)) = asked.recv().await {
                    if let Request::Editor { path } = &request {
                        let _ = reply.send(take_input(path, &mut pull, prompts, notify).await);
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
                    let worktrees: Vec<PathBuf> = tree
                        .peek()
                        .worktrees()
                        .map(|w| w.worktree.path.clone())
                        .collect();
                    let known = shells.peek().clone();
                    let daemon = hub.daemon.clone();
                    let response = answer(&request, &known, &worktrees, async |s| {
                        daemon.visible(s.id).await.ok()
                    })
                    .await;
                    let _ = reply.send(response);
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
                "const c = document.querySelector('.console');
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
                        status.set(Some(format!("cannot start Neovim: {e}")));
                        return;
                    }
                };
                let born = Instant::now();
                nvim.set(Some(embedded.clone()));
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
                            let rows = h.clamp(s.prompt_min_rows, s.prompt_max_rows);
                            if rows != *prompt_rows.peek() {
                                prompt_rows.set(rows);
                                let cols = *prompt_cols.peek();
                                let _ = embedded.resize(cols, rows as u16).await;
                            }
                        }
                    }
                }
                nvim.set(None);
                // One that quits as it starts would be started forever.
                if born.elapsed() < NVIM_SETTLE {
                    status.set(Some(
                        "Neovim quit as it started; restart adroit to try again".into(),
                    ));
                    return;
                }
                notify.notice("Neovim quit; the prompt starts afresh".into());
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
            let pane = ensure_pane(&hub, &tab, size, move || {
                *attached.write() += 1;
                if matches!(me, Tab::Session(_)) {
                    stale.write().insert(me);
                }
            });
            if let Some(pane) = pane {
                pane.resize(size);
            }
        }
    });

    // Keep what is selected in the sidebar in view, after it is drawn.
    use_effect(move || {
        let _ = selected();
        document::eval(
            "requestAnimationFrame(() => document.querySelectorAll('.sidebar .list-row.selected')
               .forEach((r) => r.scrollIntoView({ block: 'nearest' })));",
        );
    });

    // So is the picker's.
    use_effect(move || {
        let _ = picking();
        document::eval(
            "requestAnimationFrame(() => document.querySelectorAll('.dialog.picker-dialog .list-row.selected')
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

    // A shell opened or closed shows in the sidebar at once, among the
    // worktrees already listed, not when the sessions are next read.
    use_effect(move || {
        let placed = tree.peek().with_shells(&shells.read());
        if *tree.peek() != placed {
            tree.set(placed);
        }
    });
    let ib = use_memo(move || inbox(&tree.read()));
    // What the sidebar shows: the tree narrowed to what the filter holds.
    let mut filter = use_signal(String::new);
    // Whether the filter's field has the keys, and the field.
    let mut filtering = use_signal(|| false);
    let mut filter_field = use_signal(|| None::<Rc<MountedData>>);
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
        let left = focus() != Area::Sidebar || menu().is_some();
        if left && *filtering.peek() {
            focus_filter(false);
        }
    });
    let shown_tree = use_memo(move || nav::narrowed(&tree.read(), &filter.read()));
    let shown_ib = use_memo(move || inbox(&shown_tree.read()));
    let items =
        use_memo(move || nav::items(&shown_tree.read(), &shown_ib.read(), &collapsed.read()));

    // A selected row that leaves the sidebar, as an inbox row does once its
    // session stops waiting, gives way to another rather than to nothing.
    use_effect(move || {
        let list = items.read();
        let Some(p) = place.peek().clone() else {
            return;
        };
        let row = nav::row_kept(
            &list,
            &tree.peek(),
            selected.peek().as_ref(),
            active.peek().as_ref(),
            &p,
        );
        if row.is_some() {
            selected.set(row);
        }
    });

    // The selection follows the tab shown, however it came to be shown, such
    // as a closed shell's tab giving way to a session's, unless it was moved
    // elsewhere by hand.
    use_effect(move || {
        let now = active.read().clone();
        if let Some(tab @ Tab::Session(_)) = &now
            && !opened.peek().contains(tab)
        {
            opened.write().insert(tab.clone());
        }
        let _ = prompts.send(Op::Show(now));
    });
    let shown_before = use_hook(|| Rc::new(RefCell::new(None::<Tab>)));
    use_effect(move || {
        let now = active.read().clone();
        let before = shown_before.replace(now.clone());
        let (Some(now), Some(p)) = (now, place.peek().clone()) else {
            return;
        };
        let row = nav::row_after_switch(
            &items.peek(),
            &tree.peek(),
            selected.peek().as_ref(),
            before.as_ref(),
            &now,
            &p,
        );
        if let Some(row) = row.filter(|row| selected.peek().as_ref() != Some(row)) {
            selected.set(Some(row));
        }
    });

    let mut open = move |item: Item| {
        // Typing stops going to the filter once a row is taken; what it
        // narrowed to stays.
        if *filtering.peek() {
            focus_filter(false);
        }
        selected.set(Some(item.clone()));
        match &item {
            Item::Workspace(_) => {
                if let Some(k) = item.fold_key() {
                    let mut c = collapsed.write();
                    if !c.remove(&k) {
                        c.insert(k);
                    }
                }
            }
            Item::Waiting(id) | Item::Session(id) => {
                if let Some(p) = nav::place_of(&tree.peek(), &item) {
                    place.set(Some(p));
                }
                active.set(Some(Tab::Session(id.clone())));
            }
            Item::Shell(id) => {
                if let Some(p) = nav::place_of(&tree.peek(), &item) {
                    place.set(Some(p));
                }
                active.set(Some(Tab::Shell(*id)));
            }
            Item::Worktree(path) => {
                let p = Place {
                    worktree: path.clone(),
                };
                let first = nav::sessions_in(&tree.peek(), &p)
                    .first()
                    .map(|s| Tab::Session(s.id.clone()));
                active.set(Some(first.unwrap_or(Tab::Draft {
                    worktree: path.clone(),
                    role: Role::Worker,
                })));
                place.set(Some(p));
            }
        }
    };

    // The tabs of the worktree in the middle.
    let tabs = use_memo(move || {
        let Some(p) = place() else {
            return Vec::new();
        };
        let t = tree.read();
        let open = opened.read();
        let mut out: Vec<(Tab, String, Option<State>, bool)> = nav::sessions_in(&t, &p)
            .into_iter()
            .filter(|s| open.contains(&Tab::Session(s.id.clone())))
            .map(|s| {
                (
                    Tab::Session(s.id.clone()),
                    s.name.clone(),
                    Some(completions.read().state(s).clone()),
                    s.reads_only(),
                )
            })
            .collect();
        for sh in shells.read().iter().filter(|s| s.worktree == p.worktree) {
            out.push((Tab::Shell(sh.id), sh.name.clone(), None, false));
        }
        if let Some(draft @ Tab::Draft { .. }) = active()
            && let Tab::Draft { worktree, role, .. } = &draft
            && *worktree == p.worktree
        {
            let title = role.draft_title().to_string();
            let reader = *role == Role::Reader;
            out.push((draft, title, None, reader));
        }
        out
    });

    // Show a tab of the worktree in the middle, and select its row.
    let mut show_tab = move |tab: Tab| {
        if let Some(p) = place.peek().as_ref()
            && let Some(row) = nav::row_of(&items.peek(), &tree.peek(), Some(&tab), p)
        {
            selected.set(Some(row));
        }
        active.set(Some(tab));
    };

    // Show a session wherever it runs, as opening its row does, but leave the
    // keys where they are.
    let mut show_session = move |id: String| {
        let Some((p, _)) = nav::find(&tree.peek(), &id) else {
            return;
        };
        place.set(Some(p));
        show_tab(Tab::Session(id));
    };

    let mut cycle_tab = move |delta: isize| {
        let next = {
            let list = tabs.peek();
            if list.is_empty() {
                return;
            }
            let at = active
                .peek()
                .as_ref()
                .and_then(|a| list.iter().position(|(t, ..)| t == a))
                .unwrap_or(0);
            list[nav::wrap(at, delta, list.len())].0.clone()
        };
        show_tab(next);
    };

    // Whether a prompt is on its way, so a second press does not send it again.
    let mut sending = use_signal(|| false);
    let submit = {
        let hub = hub.clone();
        move || {
            let hub = hub.clone();
            if *sending.peek() || !shows_prompt(active.peek().as_ref()) {
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
                if let Some(id) = passes_enter(taken.as_ref()) {
                    let size = *console_size.peek();
                    if let Some(p) = pane_for(&hub, &Tab::Session(id), size).await {
                        p.key(&Key::new("Enter").ctrl());
                    }
                }
                let Some((shown, text)) = taken.filter(|(_, text)| !text.trim().is_empty()) else {
                    sending.set(false);
                    return;
                };
                let nv = nvim.peek().clone();
                if let Some(nv) = nv {
                    let _ = nv.stop_writing().await;
                }
                // Sent with nothing open, a prompt starts a session where it
                // can only mean, or else a scratch session, as a draft there
                // would.
                let tab = match shown {
                    None => {
                        let last = place.peek().clone();
                        let p = nav::start_place(&tree.peek(), last.as_ref(), &scratch_dir());
                        let draft = Tab::Draft {
                            worktree: p.worktree.clone(),
                            role: Role::Worker,
                        };
                        place.set(Some(p));
                        active.set(Some(draft.clone()));
                        draft
                    }
                    Some(shown) => shown,
                };
                let sent_to = Some(tab.clone());
                let size = *console_size.peek();
                let t = tree.peek().clone();
                let agent = agents::for_tab(
                    &t,
                    Some(&tab),
                    &draft_agents.peek(),
                    last_agent.peek().unwrap_or(settings::current().agent),
                );
                let waiting = |id: &str| {
                    nav::find(&t, id).is_some_and(|(_, s)| matches!(s.state, State::Waiting(_)))
                };
                let sent = match tab {
                    // The Enter that ends a prompt would pick the choice in
                    // front, usually Yes; the prompt waits for it to be answered.
                    Tab::Session(id) if waiting(&id) => {
                        // The keys go where the answer is given; what was written
                        // waits in the prompt for after.
                        focus.set(Area::Console);
                        notify.notice(
                            "Answer the session above first; your message waits here".into(),
                        );
                        false
                    }
                    tab @ Tab::Session(_) => match pane_for(&hub, &tab, size).await {
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
                                status.set(Some(
                                    "The session ended before it took the prompt".into(),
                                ));
                                false
                            } else {
                                match adroit_claude::shell_line(&text)
                                    .filter(|_| agents::adapter(agent).capabilities().shell_bang)
                                {
                                    Some(command) => {
                                        p.type_text("!");
                                        tokio::time::sleep(Duration::from_millis(40)).await;
                                        p.paste(command);
                                    }
                                    None => p.paste(&text),
                                }
                                tokio::time::sleep(Duration::from_millis(40)).await;
                                p.key(&Key::new("Enter"));
                                true
                            }
                        }
                        None => {
                            status.set(Some("This session cannot be reached from here".into()));
                            false
                        }
                    },
                    Tab::Shell(_) => false,
                    Tab::Draft { .. }
                        if named(&text).0.is_some() && named(&text).1.trim().is_empty() =>
                    {
                        notify.notice("Write what the session is to do below its name".into());
                        false
                    }
                    // Given as the first prompt, a shell line is not known to
                    // run as a command; it could reach the model as a message.
                    Tab::Draft { .. }
                        if agents::adapter(agent).capabilities().shell_bang
                            && adroit_claude::shell_line(&text).is_some() =>
                    {
                        notify.notice(
                            "A shell line runs in a session: start it with a message first".into(),
                        );
                        false
                    }
                    Tab::Draft { worktree, role } => {
                        let (given, text) = named(&text);
                        let label = given.unwrap_or_else(|| label_from(&text));
                        let taken: Vec<&str> = t.sessions().map(|(_, s)| s.name.as_str()).collect();
                        let name = fresh_name(&label, &taken);
                        let reader = role == Role::Reader;
                        let draft = Tab::Draft {
                            worktree: worktree.clone(),
                            role,
                        };
                        // Made only now, so a draft left unsent leaves no
                        // empty directory behind.
                        let started =
                            match scratch::start_dir(&worktree, &scratch_dir(), Moment::now()) {
                                Ok(worktree) => {
                                    let (conversation, (program, args, env)) =
                                        agents::start(agent, &name, reader, &text, session_env());
                                    let spawn = Spawn {
                                        program,
                                        args,
                                        cwd: worktree.clone(),
                                        env,
                                        tags: sessions::agent_tags(
                                            agent,
                                            &name,
                                            &worktree,
                                            &conversation,
                                            reader,
                                            now_ms(),
                                            None,
                                        ),
                                        cols: size.cols,
                                        rows: size.rows,
                                    };
                                    start(&hub, spawn, size, |id| Tab::Session(sessions::hex(id)))
                                        .await
                                        .map_err(|e| format!("cannot start {name}: {e}"))
                                }
                                Err(e) => Err(format!("cannot make a directory for {name}: {e}")),
                            };
                        match started {
                            Ok(id) => {
                                let id = sessions::hex(id);
                                draft_agents.write().remove(&draft);
                                hub.refresh.notify_one();
                                selected.set(Some(Item::Session(id.clone())));
                                active.set(Some(Tab::Session(id)));
                                true
                            }
                            Err(e) => {
                                status.set(Some(e));
                                false
                            }
                        }
                    }
                };
                if sent {
                    let _ = prompts.send(Op::Sent);
                } else {
                    let lines: Vec<String> = text.lines().map(String::from).collect();
                    let _ = prompts.send(Op::Add(sent_to, lines, None));
                }
                sending.set(false);
            });
        }
    };

    let mut choose_agent = move |tab: Option<Tab>, agent| {
        agents::choose(
            &mut draft_agents.write(),
            tab,
            agent,
            &mut last_agent.write(),
        );
    };

    // A session started already keeps its own agent.
    let mut switch_agent = move || match active.peek().clone() {
        tab if agents::can_choose(tab.as_ref()) => {
            let now = agents::for_tab(
                &tree.peek(),
                tab.as_ref(),
                &draft_agents.peek(),
                last_agent.peek().unwrap_or(settings::current().agent),
            );
            choose_agent(tab, now.next());
        }
        _ => notify.notice("Only a session not started yet can change its agent".into()),
    };

    // Stop a session in its terminal and resume it here.
    let take_over = {
        let hub = hub.clone();
        move |asked: Session| {
            let hub = hub.clone();
            spawn(async move {
                status.set(Some(format!("taking over {}…", asked.name)));
                // The session may have moved on while the dialog was open.
                let adapter = agents::adapter(asked.agent);
                let now = match adapter.list_sessions().await {
                    Ok(list) => list
                        .into_iter()
                        .find(|s| s.session_id == asked.session_id && s.pid == asked.pid),
                    Err(e) => {
                        status.set(Some(e.to_string()));
                        return;
                    }
                };
                let Some(now) = now.filter(Session::can_take_over) else {
                    status.set(Some(format!(
                        "{} changed in its terminal; nothing was stopped",
                        asked.name
                    )));
                    return;
                };
                let Some(resume) = adapter.resume_args(&settings::current(), &now.session_id)
                else {
                    status.set(Some("This agent cannot be taken over".into()));
                    return;
                };
                moving.write().insert(now.id.clone());
                let stopped = match now.pid {
                    Some(pid) => adroit_claude::stop(pid, adroit_claude::STOP_GRACE).await,
                    None => Ok(()),
                };
                if let Err(e) = stopped {
                    moving.write().remove(&now.id);
                    status.set(Some(format!("cannot take over {}: {e}", asked.name)));
                    return;
                }
                let size = *console_size.peek();
                let mut args = adapter.integration_args(&adroit_program());
                args.extend(resume);
                let (program, args, env) = agents::launch(now.agent, args, session_env());
                let spawn = Spawn {
                    program,
                    args,
                    cwd: now.cwd.clone(),
                    env,
                    tags: sessions::agent_tags(
                        now.agent,
                        &now.name,
                        &now.cwd,
                        &now.session_id,
                        false,
                        now_ms(),
                        None,
                    ),
                    cols: size.cols,
                    rows: size.rows,
                };
                match start(&hub, spawn, size, |id| Tab::Session(sessions::hex(id))).await {
                    Ok(id) => {
                        status.set(None);
                        let id = sessions::hex(id);
                        if *active.peek() == Some(Tab::Session(now.id.clone())) {
                            active.set(Some(Tab::Session(id.clone())));
                        }
                        if matches!(selected.peek().as_ref(), Some(Item::Session(s) | Item::Waiting(s)) if *s == now.id)
                        {
                            selected.set(Some(Item::Session(id)));
                        }
                    }
                    Err(e) => status.set(Some(format!("cannot resume {}: {e}", asked.name))),
                }
                moving.write().remove(&now.id);
                hub.refresh.notify_one();
            });
        }
    };

    // Open the picker on the sessions and the directories they run in; those
    // Claude Code was used in, and ghq's repositories and roots, follow once
    // they are read.
    let mut open_picker = move || {
        let running: Vec<PathBuf> = tree
            .peek()
            .worktrees()
            .filter(|w| !w.sessions.is_empty())
            .map(|w| w.worktree.path.clone())
            .collect();
        offers.set(picker::candidates(&running, &[], &[], &[]));
        // The home folder starts open: nearly every pick in the tree begins
        // inside it.
        let mut filer = Filer::new([home()]);
        filer.open(read_dirs);
        picking.set(Some(Picker::new(filer)));
        spawn(async move {
            // Claude Code's state file can run to megabytes, and every
            // directory it names is looked at: not on the window's thread.
            let home = home();
            let asked = settings::current().picker;
            let listed_roots = asked.root_paths(&home);
            let (recent, (ghq, ghq_roots), listed) = tokio::join!(
                tokio::task::spawn_blocking(move || places::recent(&home)),
                async {
                    if asked.ghq {
                        tokio::join!(places::ghq_list(), places::ghq_roots())
                    } else {
                        (Vec::new(), Vec::new())
                    }
                },
                tokio::task::spawn_blocking({
                    let roots = listed_roots.clone();
                    move || picker::folders_in(&roots)
                }),
            );
            let roots: Vec<PathBuf> = ghq_roots.into_iter().chain(listed_roots).collect();
            // Closed while they were read, the picker wants them no more.
            if picking.peek().is_none() {
                return;
            }
            let recent = recent.unwrap_or_default();
            let next = picker::candidates(&running, &recent, &ghq, &listed.unwrap_or_default());
            // The rows rank anew with more to rank: the cursor stays on the
            // row it was on, not on whatever comes to sit at its place.
            let (query, on) = {
                let open = picking.peek();
                let Some(p) = open.as_ref() else { return };
                let on = picked_rows.peek().get(p.cursor).cloned();
                (p.query.clone(), on)
            };
            let cursor = on.and_then(|on| {
                let sessions = picker::sessions(&tree.peek());
                picker::choices(&sessions, &next, &query, &crate::paths::home(), PICKS)
                    .iter()
                    .position(|c| *c == on)
            });
            offers.set(next);
            // The tree may already be open and moved through: its roots are
            // added to, so what was opened stays open.
            if let Some(p) = picking.write().as_mut() {
                p.filer.add_roots(roots);
                p.cursor = cursor.unwrap_or(0);
            }
        });
    };

    // A directory chosen in the picker: a new session's draft there.
    let mut choose = move |dir: PathBuf| {
        picking.set(None);
        place.set(Some(Place {
            worktree: dir.clone(),
        }));
        active.set(Some(Tab::Draft {
            worktree: dir,
            role: Role::Worker,
        }));
        focus.set(Area::Prompt);
        start_writing(nvim);
    };

    // A row of the picker's list chosen, by its place among those drawn.
    let mut choose_row = move |at: usize| {
        let chosen = picked_rows.peek().get(at).cloned();
        match chosen {
            Some(Choice::Scratch) => choose(scratch_dir()),
            Some(Choice::Directory(c)) => choose(c.path),
            Some(Choice::Session(s)) => {
                picking.set(None);
                show_session(s.id);
            }
            None => {}
        }
    };

    // The settings the Settings window changes are applied as they change.
    use_hook({
        let hub = hub.clone();
        move || {
            let _ = dioxus::desktop::window().webview.zoom(*zoomed.peek());
            spawn(async move {
                let mut changed = settings::CHANGED.subscribe();
                let mut was = settings::current();
                while changed.recv().await.is_ok() {
                    let s = settings::current();
                    if s.zoom != was.zoom {
                        zoomed.set(s.zoom);
                        let _ = dioxus::desktop::window().webview.zoom(s.zoom);
                    }
                    let face = |s: &settings::Settings| {
                        (s.font_family.clone(), s.font_size, s.font_weight)
                    };
                    if face(&s) != face(&was) || s.agent != was.agent {
                        font.set(s.clone());
                    }
                    // A new theme reaches every console open, what is drawn and
                    // what a program asking for its colours is told.
                    if s.theme != was.theme {
                        match themes::palette(&s.theme, &settings::Settings::themes_dir()) {
                            Ok(p) => {
                                for pane in hub.panes.borrow().values() {
                                    pane.set_palette(&p);
                                }
                                let mut palette = hub.palette;
                                palette.set(p);
                            }
                            Err(e) => status.set(Some(e)),
                        }
                    }
                    let rows = prompt_height
                        .peek()
                        .clamp(s.prompt_min_rows, s.prompt_max_rows);
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
                    Ok(()) => notify.notice(format!("ended {}", asked.name)),
                    Err(e) => status.set(Some(format!("cannot end {}: {e}", asked.name))),
                }
                hub.refresh.notify_one();
            });
        }
    };

    // Ask to take over an interactive session: the selected one when
    // `selection` and a session is selected, else the one the console shows.
    let mut ask_take_over = move |selection: bool| {
        let t = tree.peek();
        let chosen = match &*selected.peek() {
            Some(Item::Session(id) | Item::Waiting(id)) if selection => Some(id.clone()),
            _ => match &*active.peek() {
                Some(Tab::Session(id)) => Some(id.clone()),
                _ => None,
            },
        };
        let Some((_, s)) = chosen.as_deref().and_then(|id| nav::find(&t, id)) else {
            return;
        };
        if s.kind != Kind::Interactive {
            notify.notice(format!("{} already runs here", s.name));
        } else if s.can_take_over() {
            confirm.set(Some(Ask::TakeOver(s.clone())));
        } else {
            notify.notice(format!(
                "{} is in the middle of a turn; take it over once it is idle",
                s.name
            ));
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
            let agent = agents::for_tab(
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
                    status.set(Some(format!("cannot rename {}: {e}", r.was)));
                    return;
                }
                hub.refresh.notify_one();
                let tab = Tab::Session(r.id.clone());
                let size = *console_size.peek();
                if let Some(command) = agents::adapter(agent).rename_command(&name)
                    && let Some(p) = pane_for(&hub, &tab, size).await
                {
                    p.paste(&command);
                    tokio::time::sleep(Duration::from_millis(40)).await;
                    p.key(&Key::new("Enter"));
                }
            });
        }
    };

    // Ask to end a session adroit runs, chosen as for a take-over.

    let mut ask_stop = move |selection: bool| {
        let t = tree.peek();
        let chosen = match &*selected.peek() {
            Some(Item::Session(id) | Item::Waiting(id)) if selection => Some(id.clone()),
            _ => match &*active.peek() {
                Some(Tab::Session(id)) => Some(id.clone()),
                _ => None,
            },
        };
        let Some((_, s)) = chosen.as_deref().and_then(|id| nav::find(&t, id)) else {
            return;
        };
        if s.kind == Kind::Interactive {
            notify.notice(format!("{} runs in a terminal; end it there", s.name));
        } else {
            confirm.set(Some(Ask::Stop(s.clone())));
        }
    };

    let active_pane = {
        let hub = hub.clone();
        move || -> Option<Arc<Pane>> {
            let tab = active.peek().clone()?;
            hub.panes.borrow().get(&tab).cloned()
        }
    };

    // Told when the console changes while it is read.
    let console_changed = use_hook(|| Rc::new(Notify::new()));
    use_effect({
        let console_changed = console_changed.clone();
        move || {
            let _ = frame();
            if reading() {
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
                            Ok(()) if lines == 1 => notify.notice("Copied a line".into()),
                            Ok(()) => notify.notice(format!("Copied {lines} lines")),
                            Err(e) => notify.notice(format!("cannot copy: {e}")),
                        },
                        _ => {}
                    }
                }
            });
            spawn(async move {
                let mut shown = None::<u64>;
                loop {
                    console_changed.notified().await;
                    if !*reading.peek() {
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
    // Another tab shown is shown live.
    use_effect(move || {
        let _ = active();
        if *reading.peek() {
            reading.set(false);
        }
    });
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
                    reading.set(true);
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

    // Whether the console shows a session, which takes only its control keys.
    let shows_session = move || matches!(&*active.peek(), Some(Tab::Session(_)));
    // Whether that session waits on a choice, which every key answers.
    let waits = move || match &*active.peek() {
        Some(Tab::Session(id)) => {
            *dialog.peek()
                || nav::find(&tree.peek(), id)
                    .is_some_and(|(_, s)| matches!(s.state, State::Waiting(_)))
        }
        _ => false,
    };
    // Claude Code reports no status for a dialog it opens itself, so the
    // console's screen tells, looked at again whenever it is redrawn.
    use_effect({
        let active_pane = active_pane.clone();
        move || {
            let _ = frame();
            let shows = match &*active.read() {
                Some(tab @ Tab::Session(_)) => {
                    let agent = agents::for_tab(
                        &tree.read(),
                        Some(tab),
                        &draft_agents.read(),
                        last_agent.read().unwrap_or(font.read().agent),
                    );
                    active_pane()
                        .is_some_and(|p| agents::adapter(agent).dialog_open(&p.screen().text()))
                }
                _ => false,
            };
            if *dialog.peek() != shows {
                dialog.set(shows);
            }
        }
    });

    // Ask the session in the console to hand its input over: Ctrl-g opens
    // Claude Code's external editor, which is `adroit editor`. Claude Code
    // never calls it while a dialog is open, so the request lapses.
    let request_pull = {
        let active_pane = active_pane.clone();
        move || {
            let Some(tab @ Tab::Session(_)) = active.peek().clone() else {
                return;
            };
            if waits()
                || *direct.peek()
                || !stale.peek().contains(&tab)
                || nvim.peek().is_none()
                || !ANSWERS.load(Ordering::Relaxed)
            {
                return;
            }
            let Some(p) = active_pane().filter(|p| !p.exited()) else {
                return;
            };
            stale.write().remove(&tab);
            let this = *pulls.peek() + 1;
            pulls.set(this);
            pull.set(Some(tab));
            p.type_text("\x07");
            spawn(async move {
                tokio::time::sleep(PULL_WAIT).await;
                if *pulls.peek() == this {
                    pull.set(None);
                }
            });
        }
    };

    // While every key went to the session, some may have landed in its input
    // rather than a choice; take them back once the keys stop going there,
    // and whenever the prompt takes the keyboard.
    let mut was_free = use_signal(|| false);
    use_effect({
        let request_pull = request_pull.clone();
        move || {
            let t = tree.read();
            let answering = match &*active.read() {
                Some(Tab::Session(id)) => {
                    nav::find(&t, id).is_some_and(|(_, s)| matches!(s.state, State::Waiting(_)))
                }
                _ => false,
            };
            let free = answering || dialog() || direct();
            let was = *was_free.peek();
            if was == free {
                return;
            }
            was_free.set(free);
            if was {
                let mut request_pull = request_pull.clone();
                spawn(async move {
                    // Let Claude Code put its input line back first.
                    tokio::time::sleep(Duration::from_millis(300)).await;
                    request_pull();
                });
            }
        }
    });
    // A shell shown takes the keys from a prompt that goes away with it.
    use_effect(move || {
        let prompt = shows_prompt(active.read().as_ref());
        let area = *focus.peek();
        let to = settled(area, prompt);
        if to != area {
            focus.set(to);
        }
    });
    let mut last_area = use_signal(|| Area::Sidebar);
    use_effect({
        let mut request_pull = request_pull.clone();
        move || {
            let area = focus();
            let before = *last_area.peek();
            if before != area {
                last_area.set(area);
                if area == Area::Prompt {
                    request_pull();
                }
            }
        }
    });

    // Where text typed or pasted now goes. A dialog open types in its own
    // field, or takes no text.
    let text_goes = move || {
        if picking.peek().is_some() || renaming.peek().is_some() || confirm.peek().is_some() {
            return Typed::Nowhere;
        }
        typed_into(*focus.peek(), shows_session(), *direct.peek(), waits())
    };

    let type_text = {
        let active_pane = active_pane.clone();
        move |text: String, pasted: bool| {
            keys::trace(|| format!("text {text:?} pasted={pasted}"));
            match text_goes() {
                Typed::Prompt => {
                    focus.set(Area::Prompt);
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
                Typed::Program => {
                    if let Some(p) = active_pane() {
                        if pasted {
                            p.paste(&text);
                        } else {
                            p.type_text(&text);
                        }
                    }
                }
                Typed::Nowhere => {}
            }
        }
    };

    // An image pasted into the prompt is saved for Claude Code to read and
    // named there. The console's program is handed an empty paste, as a
    // terminal hands on an image paste: Claude Code then reads the image off
    // the clipboard itself.
    let paste_image = {
        let mut type_text = type_text.clone();
        move |mime: String, data: String| match text_goes() {
            Typed::Prompt => {
                let dir = pasted::dir();
                let now = std::time::SystemTime::now();
                pasted::prune(&dir, now);
                match pasted::save(&dir, &mime, &data, now) {
                    Ok(path) => type_text(pasted::reference(&path), true),
                    Err(e) => notify.notice(e.to_string()),
                }
            }
            Typed::Program => type_text(String::new(), true),
            Typed::Nowhere => {}
        }
    };

    // Run an action where the keys are now: what a binding runs, and what a
    // click on a row's or a tab's menu runs. `key` is the key that ran it,
    // which some actions pass on; a click has none.
    let perform = {
        let active_pane = active_pane.clone();
        let mut submit = submit.clone();
        let hub = hub.clone();
        move |action: Action, key: Option<Key>| {
            let area = *focus.peek();
            let pane = active_pane().filter(|_| area == Area::Console);
            let write = |p: &Pane, k: &Key| {
                p.key(k);
            };
            // A key sent as it is may type into Claude Code's input.
            let mut typed = || {
                if let Some(tab @ Tab::Session(_)) = active.peek().clone() {
                    stale.write().insert(tab);
                }
            };
            let nvim_insert = nvim
                .peek()
                .as_ref()
                .is_some_and(|n| n.state().mode().starts_with("insert"));
            let list = items.peek().clone();
            let cur = selected.peek().clone();
            // From the sidebar an action is on the selected row; from elsewhere,
            // on what the console shows.
            let on_list = matches!(area, Area::Sidebar | Area::Details);
            let scroll = |by: &str| {
                document::eval(&format!(
                    "const b = document.querySelector('.details-scroll'); if (b) b.scrollBy(0, {by});"
                ));
            };
            // In the details panel, the cursor moves through a list shown and
            // scrolls anything else.
            let details_list = (area == Area::Details)
                .then(|| {
                    let d = drill.peek();
                    if d.sheet() == Sheet::Overview {
                        return overview_listing(
                            overview_links.peek().as_ref(),
                            active.peek().as_ref(),
                            &folded_groups.peek(),
                            &opened_groups.peek(),
                            &tree.peek(),
                            &gallery.peek(),
                        );
                    }
                    git_view
                        .peek()
                        .as_ref()
                        .filter(|v| d.spot().as_ref() == Some(&v.spot) && v.listing.is_list())
                        .map(|v| v.listing.clone())
                })
                .flatten();
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
            let prompt = shows_prompt(active.peek().as_ref());
            let mut window = |d: Dir| {
                let to = moved(area, d, prompt);
                let l = *layout.peek();
                let hidden = (to == Area::Sidebar && !l.show_sidebar)
                    || (to == Area::Details && !l.show_details);
                if !hidden {
                    focus.set(to);
                }
            };
            match action {
                Action::None => {}
                Action::WindowLeft => window(Dir::Left),
                Action::WindowDown => window(Dir::Down),
                Action::WindowUp => window(Dir::Up),
                Action::WindowRight => window(Dir::Right),
                // The tabs of the area the keys are in: the details panel's
                // sheets there, the console's tabs elsewhere.
                Action::TabNext | Action::TabPrev => {
                    let delta = if action == Action::TabNext { 1 } else { -1 };
                    if area == Area::Details {
                        let s = drill.peek().sheet().step(delta);
                        drill.set(Drill::new(s));
                    } else {
                        cycle_tab(delta);
                    }
                }
                Action::ConsoleSend(chord) => {
                    if let Some(p) = &pane
                        && let Some(k) = chord.map(|c| c.key()).or(key)
                    {
                        write(p, &k);
                        // Control keys fill the input too: Up brings back an
                        // earlier prompt, Ctrl-_ undoes a clear.
                        typed();
                    }
                }
                Action::ConsoleSendNext => send_next.set(true),
                Action::ConsolePassEveryKey => {
                    if pane.is_some() {
                        direct.set(true);
                    }
                }
                Action::PromptFocus
                | Action::PromptWrite
                | Action::PromptSend
                | Action::HistoryPrev
                | Action::HistoryNext
                    if !prompt => {}
                Action::PromptFocus => focus.set(Area::Prompt),
                Action::PromptWrite => {
                    focus.set(Area::Prompt);
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
                    let tab = active.peek().clone();
                    let project = match &tab {
                        Some(Tab::Session(id)) => {
                            nav::find(&tree.peek(), id).map(|(_, s)| PathBuf::from(&s.cwd))
                        }
                        Some(Tab::Draft { worktree, .. }) => Some(PathBuf::from(worktree)),
                        _ => None,
                    };
                    let agent = agents::for_tab(
                        &tree.peek(),
                        tab.as_ref(),
                        &draft_agents.peek(),
                        last_agent.peek().unwrap_or(settings::current().agent),
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
                    focus.set(Area::Prompt);
                    start_writing(nvim);
                }
                Action::SessionAgentNext => switch_agent(),
                Action::SessionTakeOver => ask_take_over(on_list),
                Action::SessionStop => ask_stop(on_list),
                Action::SessionRename => {
                    let chosen = match &cur {
                        Some(Item::Session(id) | Item::Waiting(id)) if on_list => Some(id.clone()),
                        _ => match &*active.peek() {
                            Some(Tab::Session(id)) => Some(id.clone()),
                            _ => None,
                        },
                    };
                    let found = chosen
                        .as_deref()
                        .and_then(|id| nav::find(&tree.peek(), id).map(|(_, s)| s.clone()));
                    match found {
                        // `/rename` typed while it asks would answer it.
                        Some(s) if matches!(s.state, State::Waiting(_)) => {
                            notify.notice(format!("Answer {} first, then rename it", s.name))
                        }
                        Some(s) if s.kind == Kind::Own => {
                            renaming.set(Some(Rename::of(&s)));
                        }
                        Some(s) => notify.notice(format!(
                            "{} takes no message from here to rename it",
                            s.name
                        )),
                        None => {}
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
                | Action::CursorOpen
                | Action::CursorShow
                | Action::CursorFold
                | Action::CursorCopy
                | Action::CursorReveal
                    if area == Area::Details =>
                {
                    let mut d = drill.peek().clone();
                    match (&action, &details_list) {
                        // A subagent opened shows no list: the keys scroll,
                        // and those that fold go back to the overview.
                        (_, Some(Listing::Links(_))) if d.agent().is_some() => match action {
                            Action::CursorFold => {
                                d.back();
                            }
                            Action::CursorDown => scroll("17"),
                            Action::CursorUp => scroll("-17"),
                            Action::CursorFirst => scroll("-b.scrollHeight"),
                            Action::CursorLast => scroll("b.scrollHeight"),
                            _ => {}
                        },
                        (_, Some(l @ Listing::Links(entries))) => {
                            let chosen = d.selected(l).map(|at| &entries[at]);
                            // A heading folds and unfolds as a workspace's row
                            // does; a link folds the group it is in.
                            let fold = match (chosen, &action) {
                                (
                                    Some(Entry::Heading { group, folded, .. }),
                                    Action::CursorOpen | Action::CursorShow,
                                ) => Some((*group, !folded)),
                                (Some(Entry::Heading { group, .. }), Action::CursorFold) => {
                                    Some((*group, true))
                                }
                                (Some(row), Action::CursorFold) => Some((row.group(), true)),
                                _ => None,
                            };
                            let how = match action {
                                Action::CursorOpen | Action::CursorShow => Some(Use::Open),
                                Action::CursorCopy => Some(Use::Copy),
                                Action::CursorReveal => Some(Use::Reveal),
                                _ => None,
                            };
                            match (fold, how, chosen) {
                                (Some((group, shut)), ..) => {
                                    let f = refold(&folded_groups.peek(), group, shut);
                                    let o = reopened(&opened_groups.peek(), group, shut);
                                    opened_groups.set(o);
                                    let now = overview_listing(
                                        overview_links.peek().as_ref(),
                                        active.peek().as_ref(),
                                        &f,
                                        &opened_groups.peek(),
                                        &tree.peek(),
                                        &gallery.peek(),
                                    );
                                    if let Some(now) = now
                                        && let Some(at) = now.heading(group)
                                    {
                                        d.select(&now, at);
                                    }
                                    folded_groups.set(f);
                                }
                                (None, Some(how), Some(Entry::Link(link))) => {
                                    if let Some(told) = use_link(link, how) {
                                        notify.notice(told);
                                    }
                                }
                                (None, Some(how), Some(Entry::Image(shot))) => {
                                    let session = overview_links
                                        .peek()
                                        .as_ref()
                                        .map(|(id, _)| id.clone())
                                        .unwrap_or_default();
                                    if let Some(told) =
                                        use_shot(shot, how, &session, &gallery.peek())
                                    {
                                        notify.notice(told);
                                    }
                                }
                                (None, Some(how), Some(Entry::Session(r))) => {
                                    match use_related(r, how) {
                                        Went::Show(id) => show_session(id),
                                        Went::Told(told) => notify.notice(told),
                                        Went::Nowhere => {}
                                    }
                                }
                                (None, Some(Use::Open), Some(Entry::More { group, .. })) => {
                                    opened_groups.write().insert(*group);
                                }
                                (None, Some(how), Some(Entry::Agent(a))) => match how {
                                    Use::Open => {
                                        d.open(l);
                                    }
                                    Use::Copy => {
                                        notify.notice(match copy_to_clipboard(&a.prompt) {
                                            Ok(()) => {
                                                format!("Copied the prompt of {}", a.description)
                                            }
                                            Err(e) => format!("cannot copy: {e}"),
                                        })
                                    }
                                    Use::Reveal => {}
                                },
                                (None, Some(_), _) => {}
                                _ => match action {
                                    Action::CursorDown => d.step(l, 1),
                                    Action::CursorUp => d.step(l, -1),
                                    Action::CursorFirst => d.first(l),
                                    Action::CursorLast => d.last(l),
                                    _ => {}
                                },
                            }
                        }
                        (Action::CursorFold, _) => {
                            d.back();
                        }
                        (Action::CursorOpen | Action::CursorShow, Some(l)) => {
                            d.open(l);
                        }
                        (Action::CursorDown, Some(l)) => d.step(l, 1),
                        (Action::CursorUp, Some(l)) => d.step(l, -1),
                        (Action::CursorFirst, Some(l)) => d.first(l),
                        (Action::CursorLast, Some(l)) => d.last(l),
                        (Action::CursorDown, None) => scroll("17"),
                        (Action::CursorUp, None) => scroll("-17"),
                        (Action::CursorFirst, None) => scroll("-b.scrollHeight"),
                        (Action::CursorLast, None) => scroll("b.scrollHeight"),
                        _ => {}
                    }
                    if d != *drill.peek() {
                        drill.set(d);
                    }
                }
                Action::CursorDown => selected.set(nav::step(&list, cur.as_ref(), 1)),
                Action::CursorUp => selected.set(nav::step(&list, cur.as_ref(), -1)),
                Action::CursorFirst => selected.set(list.first().cloned()),
                Action::CursorLast => selected.set(list.last().cloned()),
                Action::CursorOpen => {
                    if let Some(item) = cur {
                        let folds = matches!(item, Item::Workspace(_));
                        open(item);
                        match &*active.peek() {
                            _ if folds => {}
                            Some(Tab::Draft { .. }) => {
                                focus.set(Area::Prompt);
                                start_writing(nvim);
                            }
                            Some(_) => focus.set(Area::Console),
                            None => {}
                        }
                    }
                }
                Action::CursorShow => {
                    if let Some(item) = cur {
                        open(item);
                    }
                }
                Action::CursorFold => {
                    if let Some(k) = cur.as_ref().and_then(Item::fold_key) {
                        collapsed.write().insert(k);
                    }
                }
                // Only the overview's links are copied or revealed.
                Action::CursorCopy | Action::CursorReveal => {}
                Action::WaitingNext | Action::WaitingPrev => {
                    let current = match &cur {
                        Some(Item::Waiting(id)) | Some(Item::Session(id)) => Some(id.as_str()),
                        _ => None,
                    };
                    let forward = action == Action::WaitingNext;
                    if let Some(id) = nav::next_waiting(&ib.peek(), current, forward) {
                        open(Item::Waiting(id));
                        // It waits on an answer, given in the console.
                        focus.set(Area::Console);
                    }
                }
                Action::SessionNext | Action::SessionPrev => {
                    let next = nav::next_session(
                        &tree.peek(),
                        active.peek().as_ref(),
                        place.peek().as_ref(),
                        action == Action::SessionNext,
                    );
                    if let Some(id) = next {
                        show_session(id);
                    }
                }
                Action::SessionPick => open_picker(),
                Action::SessionScratch => choose(scratch_dir()),
                Action::SessionNew | Action::SessionNewReader => {
                    let p = cur
                        .as_ref()
                        .filter(|_| on_list)
                        .and_then(|c| nav::place_of(&tree.peek(), c))
                        .or_else(|| place.peek().clone());
                    if let Some(p) = p {
                        let role = if action == Action::SessionNewReader {
                            Role::Reader
                        } else {
                            Role::Worker
                        };
                        active.set(Some(Tab::Draft {
                            worktree: p.worktree.clone(),
                            role,
                        }));
                        place.set(Some(p));
                        focus.set(Area::Prompt);
                        start_writing(nvim);
                    }
                }
                Action::ShellOpen | Action::NeovimOpen => {
                    let runs = if action == Action::NeovimOpen {
                        Runs::Neovim
                    } else {
                        Runs::Shell
                    };
                    let p = cur
                        .as_ref()
                        .filter(|_| on_list)
                        .and_then(|c| nav::place_of(&tree.peek(), c))
                        .or_else(|| place.peek().clone());
                    if let Some(p) = p {
                        let names: Vec<String> = shells
                            .peek()
                            .iter()
                            .filter(|s| s.worktree == p.worktree)
                            .map(|s| s.name.clone())
                            .collect();
                        let taken: Vec<&str> = names.iter().map(String::as_str).collect();
                        let (base, (program, args, env)) = match runs {
                            Runs::Shell => ("shell", programs::shell()),
                            Runs::Neovim => ("nvim", programs::neovim()),
                        };
                        let name = fresh_name(base, &taken);
                        let size = *console_size.peek();
                        let spawn_shell = Spawn {
                            program,
                            args,
                            cwd: p.worktree.clone(),
                            env: env.into_iter().chain(daemon::terminal_env()).collect(),
                            tags: sessions::shell_tags(&name, &p.worktree, runs),
                            cols: size.cols,
                            rows: size.rows,
                        };
                        let hub = hub.clone();
                        spawn(async move {
                            match start(&hub, spawn_shell, size, Tab::Shell).await {
                                Ok(id) => {
                                    shells.write().push(Shell {
                                        id,
                                        name,
                                        worktree: p.worktree.clone(),
                                        runs,
                                    });
                                    place.set(Some(p));
                                    active.set(Some(Tab::Shell(id)));
                                    focus.set(Area::Console);
                                }
                                Err(e) => status.set(Some(format!("cannot start {base}: {e}"))),
                            }
                        });
                    }
                }
                // On a shell's row, that shell closes, whichever tab is shown.
                Action::ShellClose | Action::TabClose
                    if let Some(Item::Shell(id)) = cur.as_ref().filter(|_| on_list) =>
                {
                    selected.set(nav::row_above(&list, &Item::Shell(*id)));
                    end_shell(&hub, *id);
                    close_shell(*id, &hub.panes, shells, active, place, tree, prompts);
                }
                Action::ShellClose => {
                    let current_tab = active.peek().clone();
                    if let Some(Tab::Shell(id)) = current_tab {
                        end_shell(&hub, id);
                        close_shell(id, &hub.panes, shells, active, place, tree, prompts);
                    }
                }
                Action::TabClose => {
                    let shown = active.peek().clone();
                    match shown {
                        Some(Tab::Shell(id)) => {
                            end_shell(&hub, id);
                            close_shell(id, &hub.panes, shells, active, place, tree, prompts);
                        }
                        // The session runs on; only its tab goes.
                        Some(tab @ Tab::Session(_)) => {
                            let listed: Vec<Tab> =
                                tabs.peek().iter().map(|(t, ..)| t.clone()).collect();
                            let next = nav::tab_after(&listed, &tab);
                            opened.write().remove(&tab);
                            let _ = prompts.send(Op::Close(next.clone()));
                            active.set(next);
                        }
                        _ => {}
                    }
                }
                Action::Refresh => hub.refresh.notify_one(),
                Action::DetailsOverview | Action::DetailsChanges | Action::DetailsLog => {
                    let s = match action {
                        Action::DetailsChanges => Sheet::Changes,
                        Action::DetailsLog => Sheet::Log,
                        _ => Sheet::Overview,
                    };
                    if drill.peek().sheet() != s {
                        drill.set(Drill::new(s));
                    }
                    let l = *layout.peek();
                    if !l.show_details {
                        let next = l.toggled(Side::Details);
                        layout.set(next);
                        next.save();
                    }
                }
                Action::WorktreesToggle => {
                    let all = !*all_worktrees.peek();
                    all_worktrees.set(all);
                    hub.refresh.notify_one();
                    notify.notice(
                        if all {
                            "Every worktree is listed, those without sessions too"
                        } else {
                            "Only the worktrees with sessions are listed"
                        }
                        .into(),
                    );
                }
                Action::SettingsOpen => settings_window::open(),
                Action::WindowClose => dioxus::desktop::window().close(),
                Action::SidebarToggle => toggle_panel(Side::Sidebar),
                Action::DetailsToggle => toggle_panel(Side::Details),
                Action::ZoomIn | Action::ZoomOut | Action::ZoomReset => {
                    let next = match action {
                        Action::ZoomIn => zoom::next(*zoomed.peek(), zoom::Step::In),
                        Action::ZoomOut => zoom::next(*zoomed.peek(), zoom::Step::Out),
                        _ => settings::current().zoom,
                    };
                    zoomed.set(next);
                    let _ = dioxus::desktop::window().webview.zoom(next);
                }
                Action::Filter => {
                    focus.set(Area::Sidebar);
                    filter.set(String::new());
                    focus_filter(true);
                }
            }
        }
    };

    // Run what a menu entry names on what the menu was opened on, as its key
    // would with that row selected or that tab shown.
    let run_menu = {
        let mut perform = perform.clone();
        move |target: Target, action: Action| {
            menu.set(None);
            match target {
                Target::Row(item) => {
                    selected.set(Some(item));
                    focus.set(Area::Sidebar);
                }
                Target::Tab(tab) => {
                    show_tab(tab);
                    focus.set(Area::Console);
                }
                Target::Link { at, .. } => {
                    let listing = overview_listing(
                        overview_links.peek().as_ref(),
                        active.peek().as_ref(),
                        &folded_groups.peek(),
                        &opened_groups.peek(),
                        &tree.peek(),
                        &gallery.peek(),
                    );
                    if let Some(l) = listing {
                        let mut d = drill.peek().clone();
                        d.select(&l, at);
                        if d != *drill.peek() {
                            drill.set(d);
                        }
                    }
                    focus.set(Area::Details);
                }
            }
            perform(action, None);
        }
    };
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
            if picking.peek().is_some() || renaming.peek().is_some() || confirm.peek().is_some() {
                return;
            }
            if action != Action::SettingsOpen {
                desktop.window.set_focus();
            }
            menu.set(None);
            pending.set(Vec::new());
            perform(action, None);
        }
    });

    // The picker's keys: those its query field does not type with, and every
    // key in its file tree.
    let picker_key = {
        let mut choose = choose;
        let mut choose_row = choose_row;
        move |key: Key| {
            let plain = !key.ctrl && !key.alt && !key.meta;
            let ctrl = key.ctrl && !key.alt && !key.meta;
            let k = key.key.as_str();
            if (plain && k == "Escape") || (ctrl && k == "c") {
                picking.set(None);
                return;
            }
            if ctrl && k == "f" {
                if let Some(p) = picking.write().as_mut() {
                    p.toggle_mode();
                }
                return;
            }
            let mode = picking.peek().as_ref().map(|p| p.mode);
            if mode == Some(Mode::List) {
                // Read before the picker is borrowed to change it: a memo
                // out of date reads the picker to work itself out again.
                let rows = picked_rows.peek().len();
                let delta = match k {
                    "ArrowDown" | "Tab" if !key.shift => Some(1),
                    "Tab" => Some(-1),
                    "n" | "j" if ctrl => Some(1),
                    "ArrowUp" => Some(-1),
                    "p" | "k" if ctrl => Some(-1),
                    _ => None,
                };
                let mut open = picking.write();
                let Some(p) = open.as_mut() else { return };
                if let Some(d) = delta {
                    p.cursor = picker::step(p.cursor, rows, d);
                } else if k == "Enter" {
                    let at = p.cursor;
                    drop(open);
                    choose_row(at);
                }
                return;
            }
            let rows = picked_tree.peek().clone();
            let mut open = picking.write();
            let Some(p) = open.as_mut() else { return };
            match k {
                "j" | "ArrowDown" if plain || key.shift => p.filer.down(&rows),
                "k" | "ArrowUp" if plain || key.shift => p.filer.up(&rows),
                "l" | "ArrowRight" if plain => p.filer.open(read_dirs),
                "h" | "ArrowLeft" if plain => p.filer.close(&rows),
                "!" => p.filer.toggle_hidden(),
                "Enter" if plain => {
                    let dir = p.filer.chosen();
                    drop(open);
                    if let Some(dir) = dir {
                        choose(dir);
                    }
                }
                _ => {}
            }
        }
    };

    let handle_key = {
        let active_pane = active_pane.clone();
        let enter_reading = enter_reading.clone();
        let mut perform = perform.clone();
        let take_over = take_over.clone();
        let mut picker_key = picker_key;
        let stop = stop.clone();
        move |key: Key| {
            if key.key == "Process"
                || key.key == "Dead"
                || key.key == "Unidentified"
                || keys::held_alone(&key)
            {
                return;
            }
            // A key closes a menu left open, and does nothing else.
            if menu.peek().is_some() {
                keys::trace(|| "dropped: a menu was open");
                menu.set(None);
                return;
            }
            // The picker holds the keys while it is open.
            if picking.peek().is_some() {
                picker_key(key);
                return;
            }
            // Renaming, the keys are its field's; none act behind it.
            if renaming.peek().is_some() {
                keys::trace(|| "dropped: renaming");
                return;
            }
            // The dialog holds the keys until it is answered.
            let asked = confirm.peek().clone();
            if let Some(ask) = asked {
                keys::trace(|| format!("to the dialog: {ask:?}"));
                match key.key.as_str() {
                    "y" | "Enter" => {
                        confirm.set(None);
                        match ask {
                            Ask::TakeOver(s) => take_over(s),
                            Ask::Stop(s) => stop(s),
                        }
                    }
                    "n" | "Escape" => confirm.set(None),
                    _ => {}
                }
                return;
            }
            let area = *focus.peek();
            let pane = active_pane().filter(|_| area == Area::Console);
            let write = |p: &Pane, k: &Key| {
                p.key(k);
            };
            // A key sent as it is may type into Claude Code's input.
            let mut typed = || {
                if let Some(tab @ Tab::Session(_)) = active.peek().clone() {
                    stale.write().insert(tab);
                }
            };
            // A Ctrl-g among keys sent as they are is the user's own: a request
            // of adroit's still waiting would take its answer.
            if (*direct.peek() || *send_next.peek()) && pull.peek().is_some() {
                pull.set(None);
            }
            // Passing every key on holds the keyboard, Ctrl-w included, until
            // Escape.
            if *direct.peek() && !key.meta {
                if let Some(p) = &pane {
                    match direct_key(&key) {
                        Some(k) => {
                            write(p, &k);
                            typed();
                        }
                        None => direct.set(false),
                    }
                    return;
                }
                direct.set(false);
            }
            if *send_next.peek() && !key.meta {
                send_next.set(false);
                if let Some(p) = &pane {
                    write(p, &key);
                    typed();
                }
                return;
            }
            // Reading, the keys are the reader's, but for those that begin a
            // global binding and those that leave it from its normal mode:
            // Escape and q for the live console, i, a and o for the prompt.
            // A key taken as global is only global: the console's binding of
            // it is for the live console.
            let mut windowing = false;
            if area == Area::Console
                && *reading.peek()
                && let Some(r) = reader.peek().clone()
            {
                windowing = !pending.peek().is_empty()
                    || key.meta
                    || keymap.peek().resolve(&[Context::Global], &[], &key) != Step::Nothing;
                if !windowing {
                    let normal = r.state().mode() == "normal";
                    let plain = !key.ctrl && !key.alt && !key.meta;
                    // A session left scrolled back would show Claude Code's
                    // jump-to-bottom line live, moving with its layout.
                    let mut leave = || {
                        reading.set(false);
                        if let Some(p) = active_pane().filter(|p| p.takes_mouse()) {
                            write(&p, &console_read::newest());
                        }
                    };
                    match key.key.as_str() {
                        "Escape" | "q" if normal && plain => {
                            leave();
                            return;
                        }
                        "i" | "a" | "I" | "A" | "o" | "O" if normal && plain => leave(),
                        _ => {
                            // A program that scrolls what it shows itself, as
                            // Claude Code on the alternate screen does, leaves
                            // nothing scrolled off to read: moving past the
                            // reader's first or last line scrolls the program,
                            // and the reader follows what it then shows. `G`
                            // and `gg` scroll it all the way, and move too.
                            let previous = read_last.peek().clone();
                            read_last.set(Some(key.clone()));
                            let program = active_pane().filter(|p| p.takes_mouse());
                            if normal
                                && let Some(p) = &program
                                && let Some(to) = console_read::to_end(&key, previous.as_ref())
                            {
                                write(p, &to);
                                *frame.write() += 1;
                            } else if normal && let Some(p) = program {
                                let screen = r.state().screen();
                                let row = screen.cursor.map_or(0, |c| c.row);
                                let edge = console_read::Edge {
                                    line: reader_view.peek().0 + row,
                                    lines: read_rows.peek().0,
                                    page: screen.rows,
                                };
                                if let Some(by) = console_read::past_edge(&key, edge) {
                                    p.wheel(by, 0, 0);
                                    *frame.write() += 1;
                                    return;
                                }
                            }
                            if let Some(n) = notation(&key) {
                                spawn(async move {
                                    let _ = r.input(&n).await;
                                });
                            }
                            return;
                        }
                    }
                }
            }
            let nvim_insert = nvim
                .peek()
                .as_ref()
                .is_some_and(|n| n.state().mode().starts_with("insert"));
            let session = shows_session();
            // A session waiting on a choice takes every key but the window's
            // own as its answer; a shell takes every key but those.
            let pane = pane.or_else(|| active_pane().filter(|_| area == Area::Prompt));
            let passing = pane.is_some() && passes(area, session, waits());
            let contexts: &[Context] = match area {
                _ if passing || windowing => &[Context::Global],
                Area::Sidebar => &[Context::Sidebar, Context::Global],
                Area::Details => &[Context::Details, Context::Sidebar, Context::Global],
                Area::Console => &[Context::Console, Context::Global],
                Area::Prompt if nvim_insert => &[Context::Prompt],
                Area::Prompt => &[Context::Prompt, Context::Global],
            };
            let contexts: Vec<_> = agents::can_choose(active.peek().as_ref())
                .then_some(Context::Draft)
                .into_iter()
                .chain(contexts.iter().copied())
                .collect();
            keys::trace(|| {
                format!(
                    "area={area:?} passing={passing} windowing={windowing} nvim_insert={nvim_insert} pending={:?}",
                    pending.peek()
                )
            });
            let seq = pending.peek().clone();
            let action = match keymap.peek().resolve(&contexts, &seq, &key) {
                Step::Wait => {
                    pending.write().push(key);
                    return;
                }
                Step::Run(action) => {
                    pending.set(Vec::new());
                    action
                }
                Step::Nothing => {
                    pending.set(Vec::new());
                    // In a session's console, a key adroit does not bind is
                    // a Vim key: it starts reading the console with it, and
                    // with a sequence it broke off, such as `g` before `g`.
                    if starts_reading(area, session, passing, *reading.peek()) && pane.is_some() {
                        let keys: Vec<String> =
                            seq.iter().chain(Some(&key)).filter_map(notation).collect();
                        enter_reading(keys);
                        return;
                    }
                    // A key that ends no binding is the area's to take, unless
                    // it broke off a sequence.
                    if !seq.is_empty() {
                        keys::trace(|| "dropped: broke off a sequence");
                        return;
                    }
                    match area {
                        _ if passing => {
                            if let Some(p) = &pane {
                                write(p, &key);
                                typed();
                            }
                        }
                        Area::Prompt => {
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
                        _ => {}
                    }
                    return;
                }
            };
            perform(action, Some(key));
        }
    };

    // Whether the page has the keys, as it last told.
    let page_focused = use_hook(|| Rc::new(Cell::new(true)));
    // Keys, text, pastes and input-method commits from the hidden text area.
    // Keys with a modifier and keys that type nothing come as keys; plain
    // characters come as text.
    use_hook({
        let mut handle_key = handle_key.clone();
        let mut type_text = type_text.clone();
        let mut paste_image = paste_image.clone();
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
                            pending.set(Vec::new());
                            type_text(text, true);
                        }
                        keys::Message::Image { mime, data } => {
                            pending.set(Vec::new());
                            paste_image(mime, data);
                        }
                        keys::Message::Text { text } if text.chars().count() == 1 => {
                            handle_key(Key::new(&text));
                        }
                        keys::Message::Text { text } => {
                            pending.set(Vec::new());
                            type_text(text, false);
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
    let reader_pictures: Vec<adroit_term::Picture> = if reading() {
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
    let current = place();
    let workspace = current
        .as_ref()
        .and_then(|p| t.workspace_of(&p.worktree))
        .map(|w| (w.name.clone(), w.root.clone(), w.scratch));
    // A workspace outside git may still be a jj repository, which the details
    // panel reads through jj.
    let outside_git = current.as_ref().is_some_and(|p| {
        t.workspace_of(&p.worktree).is_some_and(|w| !w.git)
            && !adroit_claude::jj::is_jj(&p.worktree)
    });
    let waiting = ib.read().len();
    let working = t
        .sessions()
        .filter(|(_, s)| s.state == State::Working)
        .count();
    let active_tab = active();
    let drill_now = drill();
    let sheet_now = drill_now.sheet();
    let current_agent = agents::for_tab(
        &t,
        active_tab.as_ref(),
        &draft_agents.read(),
        last_agent.read().unwrap_or(font.read().agent),
    );
    let usage_now = match usage_limits
        .read()
        .get(&current_agent)
        .and_then(|fresh| fresh.get())
    {
        Some(limits) if shows_usage(active_tab.as_ref(), current_agent) => gauges(limits, now_ms()),
        _ => Vec::new(),
    };
    let system_gauges = system_snapshot.read().gauges(&font.read().usage);
    let spot_now = spot();
    let git_now = git_view()
        .filter(|v| {
            spot_now.as_ref() == Some(&v.spot)
                && current.as_ref().is_some_and(|p| p.worktree == v.worktree)
        })
        .map(|v| v.listing);
    let crumbs = drill_now.crumbs();
    let selected_row = git_now.as_ref().and_then(|l| drill_now.selected(l));
    // A row clicked opens as `Enter` on it would.
    let mut open_row = move |i: usize| {
        let mut d = drill.peek().clone();
        let opened = git_view
            .peek()
            .as_ref()
            .filter(|v| d.spot().as_ref() == Some(&v.spot))
            .is_some_and(|v| d.open_at(&v.listing, i));
        if opened {
            drill.set(d);
        }
    };
    let target = match &active_tab {
        Some(Tab::Session(id)) => nav::find(&t, id)
            .map(|(_, s)| s.name.clone())
            .unwrap_or_else(|| id.clone()),
        Some(Tab::Shell(n)) => shells
            .read()
            .iter()
            .find(|s| s.id == *n)
            .map(|s| s.name.clone())
            .unwrap_or_default(),
        Some(Tab::Draft { role, .. }) => role.draft_title().into(),
        // A prompt sent now starts a session where it can only mean, or else
        // a scratch session.
        None => {
            let scratch = scratch_dir();
            let p = nav::start_place(&t, current.as_ref(), &scratch);
            if p.worktree == scratch {
                "new scratch session".into()
            } else {
                format!("new session in {}", short(&p.worktree))
            }
        }
    };
    // The crumbs already name the worktree.
    let crumb = match (&active_tab, target.strip_prefix("new session in ")) {
        (None, Some(_)) => "new session".to_string(),
        _ => target.clone(),
    };
    let prompt_shown = shows_prompt(active_tab.as_ref());
    let composer_room = if prompt_shown {
        composer_height() + 22.0
    } else {
        0.0
    };
    let mode = prompt_mode();
    // Whether what has the keys takes text, which an input method may compose.
    let passing = direct();
    let session_shown = matches!(active_tab, Some(Tab::Session(_)));
    let shown_id = match &active_tab {
        Some(Tab::Session(id)) => Some(id.clone()),
        _ => None,
    };
    let shown_session = match &active_tab {
        Some(Tab::Session(id)) => nav::find(&t, id).map(|(_, s)| s.clone()),
        _ => None,
    };
    let overview_now = if sheet_now == Sheet::Overview {
        overview_listing(
            overview_links().as_ref(),
            active_tab.as_ref(),
            &folded_groups(),
            &opened_groups(),
            &t,
            &gallery.read(),
        )
    } else {
        None
    };
    let link_selected = overview_now.as_ref().and_then(|l| drill_now.selected(l));
    let opened_agent = drill_now.agent().and_then(|call| {
        let read = overview_links();
        read.as_ref()?
            .1
            .agents
            .iter()
            .find(|a| a.call == call)
            .cloned()
    });
    let link_sections = {
        let entries = match &overview_now {
            // A subagent opened stands in for the rows.
            Some(Listing::Links(entries)) if drill_now.agent().is_none() => entries.as_slice(),
            _ => &[],
        };
        let worktree = current
            .as_ref()
            .map(|p| p.worktree.clone())
            .or_else(|| shown_session.as_ref().map(|s| s.cwd.clone()))
            .unwrap_or_default();
        link_sections(entries, &worktree, &home())
    };
    // A row clicked does what `Enter` on it would: a link opens, a heading
    // folds or unfolds its group.
    let mut click_link = move |at: usize| {
        let Some(l) = overview_listing(
            overview_links.peek().as_ref(),
            active.peek().as_ref(),
            &folded_groups.peek(),
            &opened_groups.peek(),
            &tree.peek(),
            &gallery.peek(),
        ) else {
            return;
        };
        let mut d = drill.peek().clone();
        d.select(&l, at);
        if d != *drill.peek() {
            drill.set(d);
        }
        let Listing::Links(entries) = &l else { return };
        match entries.get(at) {
            Some(Entry::Link(link)) => {
                if let Some(told) = use_link(link, Use::Open) {
                    notify.notice(told);
                }
            }
            Some(Entry::Image(shot)) => {
                let session = overview_links
                    .peek()
                    .as_ref()
                    .map(|(id, _)| id.clone())
                    .unwrap_or_default();
                if let Some(told) = use_shot(shot, Use::Open, &session, &gallery.peek()) {
                    notify.notice(told);
                }
            }
            Some(Entry::Session(r)) => match use_related(r, Use::Open) {
                Went::Show(id) => show_session(id),
                Went::Told(told) => notify.notice(told),
                Went::Nowhere => {}
            },
            Some(Entry::Agent(_)) => {
                let mut d = drill.peek().clone();
                if d.open(&l) {
                    drill.set(d);
                }
            }
            Some(Entry::Heading { group, folded, .. }) => {
                let f = refold(&folded_groups.peek(), *group, !folded);
                let o = reopened(&opened_groups.peek(), *group, !folded);
                opened_groups.set(o);
                folded_groups.set(f);
            }
            Some(Entry::More { group, .. }) => {
                opened_groups.write().insert(*group);
            }
            None => {}
        }
    };
    let in_dialog = session_shown && dialog();
    let answering = match &active_tab {
        Some(Tab::Session(id)) => {
            in_dialog
                || nav::find(&t, id).is_some_and(|(_, s)| matches!(s.state, State::Waiting(_)))
        }
        _ => false,
    };
    let takes_text = match area {
        // A dialog's text is typed in its own field.
        _ if picking.read().is_some() || renaming().is_some() => false,
        _ if confirm().is_some() => false,
        Area::Console => passing || answering || !session_shown,
        Area::Prompt => answering || matches!(mode.0, "insert" | "cmd"),
        Area::Sidebar | Area::Details => false,
    };
    let (footer_mode, footer_class) = match area {
        // The picker holds every key while it is open, the console's too.
        _ if picking.read().is_some() => ("PICK".to_string(), "normal"),
        Area::Console if passing => ("DIRECT".to_string(), "pass"),
        Area::Console if answering || matches!(active_tab, Some(Tab::Shell(_))) => {
            ("PASS".to_string(), "pass")
        }
        Area::Console if reading() => {
            let _ = reader_frame();
            let m = reader()
                .map(|r| r.state().mode().to_string())
                .unwrap_or_default();
            let word = if m.starts_with("visual") || m.starts_with("select") {
                "VISUAL"
            } else {
                "READ"
            };
            (word.to_string(), "normal")
        }
        Area::Console => ("CONSOLE".to_string(), "normal"),
        Area::Prompt if answering => ("PASS".to_string(), "pass"),
        Area::Prompt => (mode.1.to_string(), mode.0),
        Area::Sidebar => ("TREE".into(), "normal"),
        Area::Details => ("DETAILS".into(), "normal"),
    };
    let items_now = items();
    let sel = selected();
    let inbox_now: Vec<InboxItem> = shown_ib.read().clone();
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
    let k_waiting_prev = km.keys_for(Context::Sidebar, &Action::WaitingPrev);
    let k_waiting_next = km.keys_for(Context::Sidebar, &Action::WaitingNext);
    // The footer names the keys that work where the keys are now.
    let here: &[Context] = match area {
        Area::Sidebar => &[Context::Sidebar, Context::Global],
        Area::Details => &[Context::Details, Context::Sidebar, Context::Global],
        Area::Console => &[Context::Console, Context::Global],
        Area::Prompt => &[Context::Prompt, Context::Global],
    };
    let k_waiting_next_here = km.keys_in(here, &Action::WaitingNext);
    let k_down = km.keys_for(Context::Sidebar, &Action::CursorDown);
    let k_up = km.keys_for(Context::Sidebar, &Action::CursorUp);
    let k_open = km.keys_for(Context::Sidebar, &Action::CursorOpen);
    let k_back = km
        .keys_in(&[Context::Details, Context::Sidebar], &Action::CursorFold)
        .unwrap_or_default();
    let k_new = km.keys_for(Context::Sidebar, &Action::SessionNew);
    let k_reader = km.keys_for(Context::Sidebar, &Action::SessionNewReader);
    let k_shell = km.keys_for(Context::Sidebar, &Action::ShellOpen);
    let k_pick = km.keys_for(Context::Sidebar, &Action::SessionPick);
    let k_take_over = km.keys_for(Context::Console, &Action::SessionTakeOver);
    let k_send = km.keys_for(Context::Prompt, &Action::PromptSend);
    let k_agent = km.keys_for(Context::Draft, &Action::SessionAgentNext);
    let k_write = km.keys_for(Context::Console, &Action::PromptWrite);
    let k_escape = km.keys_for(
        Context::Console,
        &Action::ConsoleSend(Chord::parse("Escape").ok()),
    );
    let k_every = km.keys_for(Context::Console, &Action::ConsolePassEveryKey);
    let k_ctrl_w = km.keys_for(
        Context::Global,
        &Action::ConsoleSend(Chord::parse("Ctrl+w").ok()),
    );

    rsx! {
        style { "{root_style}" }
        div { class: "app",
            textarea { id: "adroit-keys", autofocus: true, readonly: !takes_text }
            div { class: "header",
                span {
                    class: if layout().show_sidebar { "pane-toggle on" } else { "pane-toggle" },
                    title: "Show or hide the sidebar (⌘B)",
                    onclick: move |_| toggle_panel(Side::Sidebar),
                    IconView { icon: Icon::LayoutSidebar, size: 16 }
                }
                div { class: "header-crumbs",
                    if let Some(p) = &current {
                        if let Some((name, _, scratch)) = &workspace {
                            span { class: "header-crumbs-item", IconView { icon: if *scratch { Icon::Notes } else { Icon::Folder }, size: 14 } "{name}" }
                            span { class: "header-crumbs-sep", IconView { icon: Icon::ChevronRight, size: 12 } }
                        }
                        span { class: "header-crumbs-item header-crumbs-path", IconView { icon: Icon::GitBranch, size: 14 } "{short(&p.worktree)}" }
                        span { class: "header-crumbs-sep", IconView { icon: Icon::ChevronRight, size: 12 } }
                        span { "{crumb}" }
                    }
                }
                div { class: "flex-spacer" }
                if waiting > 0 {
                    span { class: "header-chip wait", span { class: "state-glyph wait", IconView { icon: Icon::Bell, size: 12 } } "{waiting} waiting" }
                }
                if working > 0 {
                    span { class: "header-chip", span { class: "state-glyph run", IconView { icon: Icon::Loader, size: 12 } } "{working} working" }
                }
                span {
                    class: if layout().show_details { "pane-toggle on" } else { "pane-toggle" },
                    title: "Show or hide the details panel (⌥⌘B)",
                    onclick: move |_| toggle_panel(Side::Details),
                    IconView { icon: Icon::LayoutSidebarRight, size: 16 }
                }
                span {
                    class: "pane-toggle",
                    title: "Settings (⌘,)",
                    onclick: move |_| settings_window::open(),
                    IconView { icon: Icon::Settings, size: 16 }
                }
            }
            div {
                class: if dragging().is_some() { "app-body dragging" } else { "app-body" },
                onresize: move |e: Event<ResizeData>| {
                    if let Ok(sz) = e.data().get_border_box_size() {
                        body_width.set(sz.width);
                    }
                },
                onmousemove: move |e: Event<MouseData>| {
                    if let Some((side, from, width)) = *dragging.peek() {
                        let x = e.client_coordinates().x;
                        let moved = if side == Side::Sidebar { x - from } else { from - x };
                        let next = layout.peek().dragged(side, width + moved, *body_width.peek());
                        layout.set(next);
                    }
                },
                onmouseup: move |_| {
                    if dragging.peek().is_some() {
                        dragging.set(None);
                        layout.peek().save();
                    }
                },
                onmouseleave: move |_| {
                    if dragging.peek().is_some() {
                        dragging.set(None);
                        layout.peek().save();
                    }
                },
                div { class: if area == Area::Sidebar { "app-panel sidebar focused" } else { "app-panel sidebar" },
                    hidden: !layout().show_sidebar,
                    style: "width:{layout().drawn(body_width()).0}px",
                    onmousedown: move |_| focus.set(Area::Sidebar),
                    if !inbox_now.is_empty() {
                        div { class: "sidebar-title",
                            span { "Waiting on you" }
                            span { class: "kbd-list",
                                if let Some(k) = k_waiting_prev.clone() { Keys { keys: k } }
                                if let Some(k) = k_waiting_next.clone() { Keys { keys: k } }
                            }
                        }
                        div { class: "list-rows sidebar-inbox",
                            for it in inbox_now.iter() {
                                {
                                    let item = Item::Waiting(it.session.id.clone());
                                    let is_sel = sel.as_ref() == Some(&item);
                                    let shown = shown_id.as_deref() == Some(it.session.id.as_str());
                                    let sub = format!(
                                        "{} · {}",
                                        it.workspace,
                                        it.wait
                                    );
                                    let mut open = open;
                                    let target = Target::Row(item.clone());
                                    let badge = row_badge(&t, &item);
                                    rsx! {
                                        div {
                                            key: "w-{it.session.id}",
                                            class: match (is_sel, shown) {
                                                (true, true) => "list-row selected shown",
                                                (true, false) => "list-row selected",
                                                (false, true) => "list-row shown",
                                                (false, false) => "list-row",
                                            },
                                            onclick: move |_| open(item.clone()),
                                            oncontextmenu: move |e: Event<MouseData>| {
                                                e.prevent_default();
                                                let at = e.client_coordinates();
                                                menu.set(Some((at.x, at.y, target.clone())));
                                            },
                                            span { class: "state-glyph wait", title: "waiting", style: "margin-top:2px",
                                                IconView { icon: Icon::Bell, size: 13 }
                                            }
                                            div { class: "sidebar-inbox-text",
                                                span { class: "list-row-name",
                                                    "{it.session.name}"
                                                    if it.session.reads_only() { Book {} }
                                                }
                                                span { class: "sidebar-inbox-sub", "{sub}" }
                                            }
                                            if let Some(label) = badge {
                                                span { class: "agent-badge", title: "Session agent", "{label}" }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                        div { class: "sidebar-divider" }
                    }
                    div { class: "sidebar-title",
                        span { "Workspaces" }
                        span { class: "sidebar-tools",
                            span {
                                class: if all_worktrees() { "sidebar-tool on" } else { "sidebar-tool" },
                                title: if all_worktrees() { "List only the worktrees with sessions" } else { "List every worktree, those without sessions too" },
                                onclick: {
                                    let mut run = run_button.clone();
                                    move |_| run(Action::WorktreesToggle)
                                },
                                IconView { icon: Icon::ListTree }
                            }
                            span {
                                class: "sidebar-tool",
                                title: "Read the sessions again",
                                onclick: {
                                    let mut run = run_button.clone();
                                    move |_| run(Action::Refresh)
                                },
                                IconView { icon: Icon::Refresh }
                            }
                            span {
                                class: "sidebar-tool",
                                title: "Start a session: choose where it works",
                                onclick: move |_| open_picker(),
                                IconView { icon: Icon::Plus }
                            }
                        }
                    }
                    div {
                        class: if filtering() { "filter-field on" } else { "filter-field" },
                        title: "Narrow the list by name; Enter keeps it, Esc clears it",
                        // A click goes on with what it holds, where `/` starts afresh.
                        onclick: move |_| focus_filter(true),
                        span { class: "filter-field-funnel", IconView { icon: Icon::Filter, size: 13 } }
                        input {
                            class: "filter-field-input",
                            placeholder: "Filter",
                            value: "{filter}",
                            autocomplete: "off",
                            spellcheck: "false",
                            "autocorrect": "off",
                            "autocapitalize": "off",
                            onmounted: move |e: MountedEvent| filter_field.set(Some(e.data())),
                            onfocus: move |_| {
                                focus.set(Area::Sidebar);
                                filtering.set(true);
                            },
                            onblur: move |_| filtering.set(false),
                            oninput: move |e: Event<FormData>| filter.set(e.value()),
                            onkeydown: move |e: Event<KeyboardData>| {
                                match keys::of_field(&e).map(|k| k.key) .as_deref() {
                                    Some("Enter") => focus_filter(false),
                                    Some("Escape") => {
                                        filter.set(String::new());
                                        focus_filter(false);
                                    }
                                    _ => {}
                                }
                            },
                        }
                    }
                    div { class: "list-rows", style: "flex:1",
                        if t.workspaces.is_empty() {
                            div { class: "list-row dim", "No sessions yet" }
                        }
                        for item in items_now.iter().filter(|i| !matches!(i, Item::Waiting(_))) {
                            {
                                let it = item.clone();
                                let is_sel = sel.as_ref() == Some(&it);
                                let mut open = open;
                                let reader = matches!(&it, Item::Session(id) if nav::find(&t, id).is_some_and(|(_, s)| s.reads_only()));
                                let (indent, class, text, meta, glyph_class) = match &it {
                                    Item::Workspace(root) => {
                                        let name = t
                                            .workspace_of(root)
                                            .map(|w| w.name.clone())
                                            .unwrap_or_else(|| short(root));
                                        (0, "list-row head", name, String::new(), None)
                                    }
                                    Item::Worktree(path) => {
                                        let node = t
                                            .worktrees()
                                            .find(|w| &w.worktree.path == path);
                                        let label = node
                                            .and_then(|w| w.worktree.branch.clone())
                                            .or_else(|| path.file_name().map(|f| f.to_string_lossy().into_owned()))
                                            .unwrap_or_else(|| short(path));
                                        let empty = node.is_none_or(|w| w.sessions.is_empty() && w.shells.is_empty());
                                        (14, if empty { "list-row dim mono vacant" } else { "list-row dim mono" }, label, String::new(), None)
                                    }
                                    Item::Session(id) => {
                                        let s = nav::find(&t, id).map(|(_, s)| s.clone());
                                        // The rows are of the narrowed tree, where a
                                        // workspace can be flat that is not in full.
                                        let under_worktree = shown_tree
                                            .read()
                                            .find(id)
                                            .is_some_and(|(ws, _, _)| !ws.flat());
                                        (
                                            if under_worktree { 28 } else { 14 },
                                            "list-row",
                                            s.as_ref().map(|s| s.name.clone()).unwrap_or_else(|| id.clone()),
                                            String::new(),
                                            s.map(|s| glyph(completions.read().state(&s))),
                                        )
                                    }
                                    Item::Shell(id) => {
                                        let under_worktree = shown_tree
                                            .read()
                                            .shell(*id)
                                            .is_some_and(|(ws, _, _)| !ws.flat());
                                        let name = t.shell(*id).map(|(_, _, s)| s.name.clone()).unwrap_or_default();
                                        (if under_worktree { 28 } else { 14 }, "list-row", name, String::new(), None)
                                    }
                                    Item::Waiting(_) => (0, "list-row", String::new(), String::new(), None),
                                };
                                let terminal = match &it {
                                    Item::Shell(id) => Some(runs_of(&shells.read(), *id)),
                                    _ => None,
                                };
                                let place_icon = match &it {
                                    Item::Workspace(root) if t.workspace_of(root).is_some_and(|w| w.scratch) => Some(Icon::Notes),
                                    Item::Workspace(_) if it.fold_key().is_some_and(|k| collapsed.read().contains(&k)) => Some(Icon::Folder),
                                    Item::Workspace(_) => Some(Icon::FolderOpen),
                                    Item::Worktree(_) => Some(Icon::GitBranch),
                                    _ => None,
                                };
                                let class = if is_sel { format!("{class} selected") } else { class.to_string() };
                                // What the console shows stands out wherever the selection is.
                                let class = match &it {
                                    Item::Session(id) if shown_id.as_deref() == Some(id.as_str()) => format!("{class} shown"),
                                    Item::Shell(id) if active_tab == Some(Tab::Shell(*id)) => format!("{class} shown"),
                                    _ => class,
                                };
                                let target = Target::Row(it.clone());
                                // Where a session can start, a button starts one there.
                                let starts = matches!(it, Item::Workspace(_) | Item::Worktree(_));
                                let has_draft = match &it {
                                    Item::Session(id) | Item::Waiting(id) => {
                                        drafted.read().contains(&Tab::Session(id.clone()))
                                    }
                                    _ => false,
                                };
                                let here = target.clone();
                                let badge = row_badge(&t, &it);
                                rsx! {
                                    div {
                                        key: "{it:?}",
                                        class: "{class}",
                                        style: "padding-left:{8 + indent}px",
                                        title: "{row_tip(&it)}",
                                        onclick: move |_| open(it.clone()),
                                        oncontextmenu: move |e: Event<MouseData>| {
                                            e.prevent_default();
                                            let at = e.client_coordinates();
                                            menu.set(Some((at.x, at.y, target.clone())));
                                        },
                                        if let Some((g, icon, tip)) = glyph_class {
                                            span { class: "{g}", title: "{tip}", IconView { icon, size: 13 } }
                                        }
                                        if let Some(icon) = place_icon {
                                            span { class: "place-mark", IconView { icon } }
                                        }
                                        if let Some(runs) = terminal { Terminal { runs } }
                                        span { class: "list-row-name", "{text}" }
                                        if reader { Book {} }
                                        if has_draft {
                                            span { class: "draft-mark", title: "Something is written for it in the prompt", "✎" }
                                        }
                                        span { class: "list-row-meta", "{meta}" }
                                        if let Some(label) = badge {
                                            span { class: "agent-badge", title: "Session agent", "{label}" }
                                        }
                                        if starts {
                                            span {
                                                class: "list-row-start",
                                                title: "Start a session here",
                                                onclick: {
                                                    let mut run = run_menu.clone();
                                                    move |e: Event<MouseData>| {
                                                        e.stop_propagation();
                                                        run(here.clone(), Action::SessionNew);
                                                    }
                                                },
                                                IconView { icon: Icon::Plus, size: 13 }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
                if layout().show_sidebar {
                    div {
                        class: "panel-splitter",
                        title: "Drag to resize the sidebar; double-click for its width as designed",
                        onmousedown: move |e: Event<MouseData>| {
                            e.stop_propagation();
                            e.prevent_default();
                            dragging.set(Some((Side::Sidebar, e.client_coordinates().x, layout.peek().drawn(*body_width.peek()).0)));
                        },
                        ondoubleclick: move |_| {
                            let next = Layout { sidebar: layout::SIDE, ..*layout.peek() };
                            layout.set(next);
                            next.save();
                        },
                    }
                }
                div { class: if matches!(area, Area::Console | Area::Prompt) { "app-panel center-panel focused" } else { "app-panel center-panel" },
                    onmousedown: move |_| focus.set(Area::Console),
                    div { class: "tabs",
                        // The worktree by name only: the header above holds its
                        // whole path, and here it would crowd the tabs out.
                        if let Some(p) = &current {
                            span { class: "tabs-where", title: "{short(&p.worktree)}",
                                "{p.worktree.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_else(|| short(&p.worktree))}"
                            }
                        }
                        for (tab , name , state , reader) in tabs().into_iter() {
                            {
                                let is_active = active_tab.as_ref() == Some(&tab);
                                let shell = match &tab {
                                    Tab::Shell(id) => Some(runs_of(&shells.read(), *id)),
                                    _ => None,
                                };
                                let class = match (is_active, shell.is_some()) {
                                    (true, true) => "tab active shell",
                                    (true, false) => "tab active",
                                    (false, true) => "tab shell",
                                    (false, false) => "tab",
                                };
                                let glyph_class = state.as_ref().map(glyph);
                                let t2 = tab.clone();
                                let target = Target::Tab(tab.clone());
                                let closing = Target::Tab(tab.clone());
                                let closes = !matches!(tab, Tab::Draft { .. });
                                let has_draft = drafted.read().contains(&tab);
                                rsx! {
                                    span {
                                        key: "{tab:?}",
                                        class: "{class}",
                                        onclick: move |_| show_tab(t2.clone()),
                                        oncontextmenu: move |e: Event<MouseData>| {
                                            e.prevent_default();
                                            let at = e.client_coordinates();
                                            menu.set(Some((at.x, at.y, target.clone())));
                                        },
                                        if let Some((g, icon, tip)) = glyph_class {
                                            span { class: "{g}", title: "{tip}", IconView { icon, size: 13 } }
                                        }
                                        if let Some(runs) = shell { Terminal { runs } }
                                        if matches!(tab, Tab::Draft { role: Role::Worker, .. }) { span { class: "place-mark", IconView { icon: Icon::MessagePlus, size: 13 } } }
                                        "{name}"
                                        if reader { Book {} }
                                        if has_draft {
                                            span { class: "draft-mark", title: "Something is written for it in the prompt", "✎" }
                                        }
                                        if closes {
                                            span {
                                                class: "tab-close",
                                                title: match shell {
                                                    Some(Runs::Shell) => "Close the shell",
                                                    Some(Runs::Neovim) => "Close Neovim",
                                                    None => "Close the tab; the session runs on",
                                                },
                                                onclick: {
                                                    let mut run = run_menu.clone();
                                                    move |e: Event<MouseData>| {
                                                        e.stop_propagation();
                                                        run(closing.clone(), Action::TabClose);
                                                    }
                                                },
                                                IconView { icon: Icon::X, size: 12 }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
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
                            (Some(Tab::Draft { worktree, role, .. }), _) => rsx! {
                                div { class: "console-empty",
                                    IconView { icon: if *role == Role::Reader { Icon::Book } else { Icon::MessagePlus }, size: 28 }
                                    div { class: "console-empty-title", "{role.draft_heading()}" }
                                    AgentChoices {
                                        current: current_agent,
                                        keys: k_agent.clone(),
                                        onchoose: move |a| {
                                            choose_agent(active.peek().clone(), a);
                                            focus.set(Area::Prompt);
                                            let writing = nvim
                                                .peek()
                                                .as_ref()
                                                .is_some_and(|n| n.state().mode().starts_with("insert"));
                                            if !writing {
                                                start_writing(nvim);
                                            }
                                        },
                                    }
                                    if *role == Role::Reader {
                                        div { "A reader reads and reports: it plans, and edits no files." }
                                    }
                                    if let Some(k) = k_send.clone() {
                                        div { "Write the first prompt below and press " Keys { keys: k } "." }
                                    }
                                    div { "Start it with a line " kbd { "name: …" } " to name the session." }
                                    div { class: "console-empty-path", "It starts in {short(worktree)}" }
                                }
                            },
                            (Some(_), true) if reading() => rsx! {
                                ReaderScreen { reader, reader_frame, cell_width: cw, focused: area == Area::Console, pictures: reader_pictures.clone(), onscroll: { let mut scroll = scroll_reader.clone(); move |lines: i32| scroll(lines) }, onclick: { let click = click_reader.clone(); move |at| click(at) } }
                            },
                            (Some(tab), true) => rsx! {
                                ConsoleScreen { tab: tab.clone(), frame, cell_width: cw, focused: area == Area::Console }
                            },
                            (Some(Tab::Session(id)), false) if nav::find(&t, id).is_some_and(|(_, s)| s.kind == Kind::Interactive) => {
                                let ready = nav::find(&t, id).is_some_and(|(_, s)| s.can_take_over());
                                rsx! {
                                    div { class: "console-empty",
                                        IconView { icon: Icon::Terminal, size: 28 }
                                        div { class: "console-empty-title", "Running in another terminal" }
                                        if ready {
                                            div { "It shows only in the terminal it was started in. Press " if let Some(k) = k_take_over.clone() { Keys { keys: k } } " to take it over here; Claude Code stops in that terminal." }
                                        } else {
                                            div { "It shows only in the terminal it was started in, and is in the middle of a turn. Press " if let Some(k) = k_take_over.clone() { Keys { keys: k } } " once it is idle to take it over here." }
                                        }
                                        div { class: "console-empty-hint", "Or press " kbd { "←" } " on its empty prompt in that terminal." }
                                    }
                                }
                            },
                            (Some(_), false) => rsx! {
                                div { class: "console-empty", "Starting…" }
                            },
                            (None, _) => rsx! {
                                div { class: "console-empty",
                                    IconView { icon: Icon::LayoutDashboard, size: 28 }
                                    div { class: "console-empty-title", "Nothing open" }
                                    AgentChoices {
                                        current: current_agent,
                                        keys: k_agent.clone(),
                                        onchoose: move |a| choose_agent(None, a),
                                    }
                                    div {
                                        "Move with "
                                        if let Some(k) = k_down.clone() { Keys { keys: k } }
                                        " "
                                        if let Some(k) = k_up.clone() { Keys { keys: k } }
                                        ", open with "
                                        if let Some(k) = k_open.clone() { Keys { keys: k } }
                                        ". On a workspace's row, start a session with "
                                        if let Some(k) = k_new.clone() { Keys { keys: k } }
                                        ", a reader with "
                                        if let Some(k) = k_reader.clone() { Keys { keys: k } }
                                        ", a shell with "
                                        if let Some(k) = k_shell.clone() { Keys { keys: k } }
                                        "."
                                    }
                                    div { class: "console-empty-start",
                                        button {
                                            class: "button primary",
                                            onclick: move |_| open_picker(),
                                            if let Some(k) = k_pick.clone() { Keys { keys: k } }
                                            "Start a session"
                                        }
                                        div { class: "console-empty-hint", "Choose the directory it works in: one sessions run in, one Claude Code was used in, a repository ghq keeps, or any from a file tree." }
                                    }
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
                                    focus.set(Area::Prompt);
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
                            }
                        }
                    }
                }
                if layout().show_details {
                    div {
                        class: "panel-splitter",
                        title: "Drag to resize the details panel; double-click for its width as designed",
                        onmousedown: move |e: Event<MouseData>| {
                            e.stop_propagation();
                            e.prevent_default();
                            dragging.set(Some((Side::Details, e.client_coordinates().x, layout.peek().drawn(*body_width.peek()).1)));
                        },
                        ondoubleclick: move |_| {
                            let next = Layout { details: layout::SIDE, ..*layout.peek() };
                            layout.set(next);
                            next.save();
                        },
                    }
                }
                div { class: if area == Area::Details { "app-panel details focused" } else { "app-panel details" },
                    hidden: !layout().show_details,
                    style: "width:{layout().drawn(body_width()).1}px",
                    onmousedown: move |_| focus.set(Area::Details),
                    div { class: "details-sheets",
                        for s in Sheet::ALL {
                            span {
                                key: "{s.title()}",
                                class: if s == sheet_now { "details-sheet active" } else { "details-sheet" },
                                title: km.keys_for(Context::Global, &sheet_action(s)).unwrap_or_default(),
                                onclick: move |_| {
                                    if drill.peek().sheet() != s {
                                        drill.set(Drill::new(s));
                                    }
                                },
                                if let Some(icon) = Icon::of(&sheet_action(s)) {
                                    IconView { icon, size: 13 }
                                }
                                "{s.title()}"
                            }
                        }
                    }
                    div { class: "details-scroll",
                    if sheet_now == Sheet::Overview {
                    if let Some(s) = &shown_session {
                        div { class: "details-block",
                            span { class: "details-label", "{s.name}" }
                            div { class: "details-facts",
                                for (label , value) in facts(s, now_ms()) {
                                    span { class: "details-facts-key", "{label}" }
                                    span { "{value}" }
                                }
                            }
                        }
                    }
                    if let Some((name, root, _)) = &workspace {
                        div { class: "details-block",
                            span { class: "details-label", "Workspace" }
                            span { "{name}" }
                            span { class: "details-path", "{root.display()}" }
                        }
                    }
                    if let Some(p) = &current {
                        div { class: "details-block",
                            span { class: "details-label", "Worktree" }
                            span { class: "details-path", "{p.worktree.display()}" }
                        }
                    }
                    if let Some(agent) = opened_agent {
                        SubagentView {
                            agent,
                            now_ms: now_ms(),
                            back_keys: k_back.clone(),
                            on_back: move |_| {
                                let mut d = drill.peek().clone();
                                if d.back() {
                                    drill.set(d);
                                }
                            },
                        }
                    }
                    for section in link_sections.into_iter() {
                        div { key: "{section.group.title()}", class: "details-block details-links",
                            {
                                let at = section.at;
                                rsx! {
                                    div {
                                        class: if link_selected == Some(at) { "list-row details-link-heading selected" } else { "list-row details-link-heading" },
                                        onclick: move |_| click_link(at),
                                        IconView { icon: if section.folded { Icon::ChevronRight } else { Icon::ChevronDown }, size: 12 }
                                        span { class: "details-label", "{section.group.title()}" }
                                        if section.folded {
                                            span { class: "details-link-tag", "{section.count}" }
                                        }
                                    }
                                }
                            }
                            for r in section.rows.into_iter() {
                                {
                                    let (at, file) = (r.at, r.file);
                                    let class = match (link_selected == Some(at), r.thumb.is_some()) {
                                        (true, true) => "list-row details-link details-image selected",
                                        (true, false) => "list-row details-link selected",
                                        (false, true) => "list-row details-link details-image",
                                        (false, false) => "list-row details-link",
                                    };
                                    rsx! {
                                        div { key: "{r.target}",
                                            class,
                                            title: "{r.target}",
                                            onclick: move |_| click_link(at),
                                            oncontextmenu: move |e: Event<MouseData>| {
                                                e.prevent_default();
                                                let p = e.client_coordinates();
                                                menu.set(Some((p.x, p.y, Target::Link { at, file })));
                                            },
                                            if let Some(thumb) = &r.thumb {
                                                img { class: "details-thumb", src: "{thumb}", alt: "" }
                                            } else if r.tone.is_empty() {
                                                IconView { icon: r.icon, size: 13 }
                                            } else {
                                                span { class: "{r.tone}", IconView { icon: r.icon, size: 13 } }
                                            }
                                            span { class: "list-row-name", "{r.label}" }
                                            if let Some(b) = &r.badge {
                                                span { class: "commit-mark details-link-badge", "{b}" }
                                            }
                                            if !r.tag.is_empty() {
                                                span { class: "details-link-tag", "{r.tag}" }
                                            }
                                        }
                                    }
                                }
                            }
                            if let Some((at, hidden)) = section.more {
                                div { key: "more-{at}",
                                    class: if link_selected == Some(at) { "list-row details-link details-more selected" } else { "list-row details-link details-more" },
                                    onclick: move |_| click_link(at),
                                    IconView { icon: Icon::Dots, size: 13 }
                                    span { class: "list-row-name", "Show all" }
                                    span { class: "details-link-tag", "{hidden} more" }
                                }
                            }
                        }
                    }
                    }
                    if spot_now.is_some() {
                        div { class: "list-rows details-git",
                            if !crumbs.is_empty() {
                                div { class: "details-crumbs",
                                    title: "Back to the list ({k_back})",
                                    onclick: move |_| {
                                        let mut d = drill.peek().clone();
                                        if d.back() {
                                            drill.set(d);
                                        }
                                    },
                                    span { class: "details-crumb-back", IconView { icon: Icon::ChevronLeft } }
                                    for (i , c) in crumbs.iter().enumerate() {
                                        if i > 0 {
                                            span { class: "details-crumb-sep", IconView { icon: Icon::ChevronRight, size: 12 } }
                                        }
                                        span { class: "details-crumb", "{c}" }
                                    }
                                }
                            }
                            match (&current, &git_now) {
                                (None, _) => rsx! { div { class: "list-row dim", "Open a worktree to see it here" } },
                                (Some(_), _) if outside_git => rsx! { div { class: "list-row dim", "Not a git or jj repository" } },
                                (Some(_), None) => rsx! { div { class: "list-row dim", "Reading…" } },
                                (Some(_), Some(Listing::Error(e))) => rsx! { div { class: "git-error", "{e}" } },
                                (Some(_), Some(Listing::Files(files))) if files.is_empty() => rsx! { div { class: "list-row dim", "No changes" } },
                                (Some(_), Some(Listing::Files(files))) => rsx! {
                                    for (i , f) in files.iter().enumerate() {
                                        div { key: "{f.path}",
                                            class: if selected_row == Some(i) { "list-row mono selected" } else { "list-row mono" },
                                            onclick: move |_| open_row(i),
                                            span { class: "details-file-status", "{f.status}" }
                                            span { class: "list-row-name", "{f.shown()}" }
                                        }
                                    }
                                },
                                (Some(_), Some(Listing::Log(commits))) => {
                                    let rows = graph::layout(commits.iter().map(|c| (c.node.as_str(), c.parents.as_slice())));
                                    let lanes = rows.iter().map(graph::Row::width).max().unwrap_or(1);
                                    rsx! {
                                    for (i , (c , row)) in commits.iter().zip(&rows).enumerate() {
                                        div { key: "{c.id}",
                                            class: if selected_row == Some(i) { "commit selected" } else { "commit" },
                                            onclick: move |_| open_row(i),
                                            div {
                                                class: "commit-graph",
                                                dangerous_inner_html: graph::svg(row, lanes, graph::Marks { current: c.current, empty: c.empty, conflict: c.conflict }),
                                            }
                                            div { class: "commit-text",
                                            span { class: "commit-hash", "{c.short}" }
                                            span { class: "commit-subject",
                                                if c.current {
                                                    span { class: "commit-mark current", "@" }
                                                }
                                                if c.conflict {
                                                    span { class: "commit-mark conflict", "conflict" }
                                                }
                                                if c.empty {
                                                    span { class: "commit-mark", "empty" }
                                                }
                                                for b in c.bookmarks.iter() {
                                                    span { class: "commit-mark bookmark", "{b}" }
                                                }
                                                if c.subject.is_empty() {
                                                    span { class: "commit-none", "(no description)" }
                                                } else {
                                                    "{c.subject}"
                                                }
                                            }
                                            span { class: "commit-when", "{c.when}" }
                                            }
                                        }
                                    }
                                    }
                                }
                                (Some(_), Some(Listing::Diff(lines))) if lines.is_empty() => rsx! { div { class: "list-row dim", "No changes" } },
                                (Some(_), Some(Listing::Diff(lines))) => rsx! {
                                    div { class: "diff",
                                        for (i , d) in lines.iter().enumerate() {
                                            div { key: "{i}", class: "{diff_class(d.kind)}", "{d.text}" }
                                        }
                                    }
                                },
                                // The git sheets never read the overview's links.
                                (Some(_), Some(Listing::Links(_))) => rsx! {},
                            }
                        }
                    }
                    }
                    if !usage_now.is_empty() {
                        div { class: "details-usage",
                            title: "Usage limits reported by the session's agent",
                            div { class: "usage-header",
                                span { class: "details-label", "Usage" }
                                if let Some(url) = agents::adapter(current_agent).usage_url().filter(|_| !usage_now.is_empty()) {
                                    button { class: "usage-web",
                                        title: "Open {agents::adapter(current_agent).label()} usage settings in your browser",
                                        onclick: move |ev| {
                                            ev.stop_propagation();
                                            let _ = use_link(&Link {
                                                kind: LinkKind::Url,
                                                target: url.into(),
                                                title: None,
                                                line: None,
                                            }, Use::Open);
                                        },
                                        IconView { icon: Icon::Link, size: 12 }
                                        "Web"
                                    }
                                }
                            }
                            for g in usage_now.into_iter() {
                                div { key: "{g.label}", class: "usage-gauge",
                                    div { class: "usage-gauge-head",
                                        span { class: "usage-gauge-label", "{g.label}" }
                                        span { class: "usage-gauge-left", "{g.left}" }
                                    }
                                    div { class: "usage-gauge-track",
                                        div {
                                            class: match g.severity {
                                                Severity::Normal => "usage-gauge-fill",
                                                Severity::Warning => "usage-gauge-fill warning",
                                                Severity::Critical => "usage-gauge-fill critical",
                                            },
                                            style: "width:{g.used}%",
                                        }
                                    }
                                    span { class: "usage-gauge-note", "{g.note}" }
                                }
                            }
                        }
                    }
                    if !system_gauges.is_empty() {
                        div { class: "details-system-usage",
                            title: "Whole-machine resource usage",
                            div { class: "system-usage-grid",
                                for g in system_gauges {
                                    div { key: "{g.label}", class: "{g.class}",
                                        div { class: "usage-resource-head",
                                            span { class: "usage-resource-label",
                                                IconView { icon: g.icon, size: 14 }
                                                "{g.label}"
                                            }
                                            span { class: "usage-resource-value", "{g.value}" }
                                        }
                                        div { class: "usage-resource-track",
                                            div { class: "usage-resource-fill", style: "width:{g.used}%" }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
            div { class: "footer",
                span { class: "mode-chip {footer_class}", title: "{mode_tip(&footer_mode, in_dialog)}", "{footer_mode}" }
                if let Some(msg) = status() {
                    span { class: "footer-status", title: "{msg}", "{msg}" }
                }
                span { class: "flex-spacer" }
                if let Some(k) = k_moves.clone() {
                    span { class: "kbd-list", Keys { keys: k } " move" }
                }
                if let Some(k) = k_waiting_next_here.clone() {
                    span { class: "kbd-list", Keys { keys: k } " next waiting" }
                }
                if area == Area::Sidebar && filtering() {
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
                } else if area == Area::Console && session_shown && reading() {
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
                    if let Some(k) = k_ctrl_w.clone() {
                        span { class: "kbd-list", Keys { keys: k } " send Ctrl-w" }
                    }
                }
            }
            if let Some(p) = picking() {
                {
                    let rows = picker::lines(&picked_rows(), &home());
                    let tree_rows = picked_tree();
                    rsx! {
                        div {
                            onclick: move |_| picking.set(None),
                            PickerView {
                                picker: p,
                                rows,
                                tree: tree_rows,
                                onchoose: move |at: usize| choose_row(at),
                                onpick: move |dir: PathBuf| choose(dir),
                                onquery: move |q: String| {
                                    if let Some(p) = picking.write().as_mut() {
                                        p.set_query(&q);
                                    }
                                },
                                onkey: {
                                    let mut key = picker_key;
                                    move |k: Key| key(k)
                                },
                            }
                        }
                    }
                }
            }
            if let Some((x, y, target)) = menu() {
                {
                    let session = match &target {
                        Target::Row(Item::Session(id) | Item::Waiting(id))
                        | Target::Tab(Tab::Session(id)) => nav::find(&t, id).map(|(_, s)| s.clone()),
                        _ => None,
                    };
                    // The keys that work wherever the keys are come first.
                    let contexts: &[Context] = match target {
                        Target::Row(_) => &[Context::Global, Context::Sidebar],
                        Target::Tab(_) => &[Context::Global, Context::Console, Context::Sidebar],
                        Target::Link { .. } => &[Context::Details, Context::Sidebar, Context::Global],
                    };
                    // Each entry with whether a separator goes above it.
                    let entries: Vec<(Action, &str, Option<String>, bool)> = menu::groups(&target, session.as_ref())
                        .into_iter()
                        .enumerate()
                        .flat_map(|(g, group)| {
                            group.into_iter().enumerate().map(move |(i, (a, label))| (a, label, g > 0 && i == 0))
                        })
                        .map(|(a, label, apart)| {
                            let keys = km.keys_in(contexts, &a);
                            (a, label, keys, apart)
                        })
                        .collect();
                    rsx! {
                        div {
                            class: "context-menu-shade",
                            onclick: move |_| menu.set(None),
                            oncontextmenu: move |e: Event<MouseData>| {
                                e.prevent_default();
                                menu.set(None);
                            },
                            div {
                                class: "context-menu",
                                style: "left:{x}px;top:{y}px",
                                onclick: move |e| e.stop_propagation(),
                                for (action , label , keys , apart) in entries {
                                    {
                                        let target = target.clone();
                                        let mut run = run_menu.clone();
                                        let act = action.clone();
                                        rsx! {
                                            Fragment { key: "{action}",
                                                if apart {
                                                    div { class: "context-menu-sep" }
                                                }
                                                div {
                                                    class: "context-menu-item",
                                                    onclick: move |_| run(target.clone(), act.clone()),
                                                    span { class: "context-menu-icon",
                                                        if let Some(icon) = Icon::of(&action) {
                                                            IconView { icon, size: 15 }
                                                        }
                                                    }
                                                    span { class: "context-menu-label", "{label}" }
                                                    if let Some(k) = keys {
                                                        span { class: "context-menu-keys", Keys { keys: k } }
                                                    }
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
            if let Some(r) = renaming() {
                {
                    let send = send_rename.clone();
                    let ready = r.name().is_some();
                    rsx! {
                        div { class: "dialog-scrim", onclick: move |_| renaming.set(None),
                            div { class: "dialog rename-dialog",
                                onclick: move |e| e.stop_propagation(),
                                div { class: "dialog-title", IconView { icon: Icon::Pencil, size: 16 } "Rename {r.was}" }
                                div { class: "filter-field on",
                                    input {
                                        class: "filter-field-input",
                                        value: "{r.text}",
                                        autocomplete: "off",
                                        spellcheck: "false",
                                        "autocorrect": "off",
                                        "autocapitalize": "off",
                                        // Selected, so a new name is typed over the old one.
                                        onmounted: move |e: MountedEvent| async move {
                                            let _ = e.data().set_focus(true).await;
                                            document::eval(
                                                "document.querySelector('.rename-dialog .filter-field-input')?.select()",
                                            );
                                        },
                                        oninput: move |e: Event<FormData>| {
                                            if let Some(r) = renaming.write().as_mut() {
                                                r.text = e.value();
                                            }
                                        },
                                        onkeydown: {
                                            let send = send_rename.clone();
                                            move |e: Event<KeyboardData>| {
                                                match keys::of_field(&e).map(|k| k.key).as_deref() {
                                                    Some("Enter") => {
                                                        if let Some(r) = renaming.take() {
                                                            send(r);
                                                        }
                                                    }
                                                    Some("Escape") => renaming.set(None),
                                                    _ => {}
                                                }
                                            }
                                        },
                                    }
                                }
                                div { class: "dialog-actions",
                                    button { class: "button", onclick: move |_| renaming.set(None),
                                        kbd { "Esc" } "Cancel"
                                    }
                                    button {
                                        class: "button primary",
                                        disabled: !ready,
                                        onclick: move |_| {
                                            if let Some(r) = renaming.take() {
                                                send(r);
                                            }
                                        },
                                        kbd { "Enter" } "Rename"
                                    }
                                }
                            }
                        }
                    }
                }
            }
            if let Some(ask) = confirm() {
                {
                    let take_over = take_over.clone();
                    let stop = stop.clone();
                    let icon = Icon::of(&match ask { Ask::TakeOver(_) => Action::SessionTakeOver, Ask::Stop(_) => Action::SessionStop });
                    let (title, body, act, s) = match ask.clone() {
                        Ask::TakeOver(s) => (
                            format!("Take over {}?", s.name),
                            "Claude Code stops in the terminal it runs in, and the conversation continues here. That terminal goes back to its shell.",
                            "Take over",
                            s,
                        ),
                        Ask::Stop(s) => (
                            format!("End {}?", s.name),
                            "Claude Code exits where it runs. Its conversation is kept, for /resume in a session started later.",
                            "End",
                            s,
                        ),
                    };
                    rsx! {
                        div { class: "dialog-scrim",
                            div { class: "dialog",
                                div { class: "dialog-title", if let Some(icon) = icon { IconView { icon, size: 16 } } "{title}" }
                                div { "{body}" }
                                div { class: "dialog-actions",
                                    button { class: "button", onclick: move |_| confirm.set(None),
                                        kbd { "n" } "Cancel"
                                    }
                                    button { class: "button primary",
                                        onclick: move |_| {
                                            confirm.set(None);
                                            match &ask {
                                                Ask::TakeOver(_) => take_over(s.clone()),
                                                Ask::Stop(_) => stop(s.clone()),
                                            }
                                        },
                                        kbd { "y" } "{act}"
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

#[component]
fn AgentChoices(
    current: adroit_model::Agent,
    keys: Option<String>,
    onchoose: EventHandler<adroit_model::Agent>,
) -> Element {
    rsx! {
        div { class: "console-agents",
            for agent in adroit_model::Agent::ALL {
                button {
                    key: "{agent.key()}",
                    class: if agent == current { "button primary" } else { "button" },
                    onclick: move |e| {
                        e.stop_propagation();
                        onchoose.call(agent);
                    },
                    "{agents::adapter(agent).label()}"
                }
            }
        }
        if let Some(keys) = keys {
            div { class: "console-empty-hint", Keys { keys } " switches the agent it runs." }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::details::overview;

    #[test]
    fn usage_depends_only_on_the_console_agent() {
        use adroit_model::Agent;

        for agent in [Agent::Claude, Agent::Codex] {
            for tab in [
                Tab::Session("session".into()),
                Tab::Draft {
                    worktree: "/project".into(),
                    role: Role::Worker,
                },
            ] {
                assert!(shows_usage(Some(&tab), agent));
            }
            assert!(!shows_usage(Some(&Tab::Shell(1)), agent));
            assert!(!shows_usage(None, agent));
        }
    }

    #[test]
    fn sidebar_session_badges_identify_the_agent_even_when_names_match() {
        use adroit_model::Agent;

        let sessions = [Agent::Claude, Agent::Codex].map(|agent| Session {
            id: agent.key().into(),
            agent,
            name: "Review".into(),
            cwd: "/project".into(),
            ..Session::default()
        });
        let tree = build(&sessions, &[], &HashMap::new(), &[], None);
        for (id, label) in [("claude", "Claude Code"), ("codex", "Codex")] {
            assert_eq!(row_badge(&tree, &Item::Session(id.into())), Some(label));
            assert_eq!(row_badge(&tree, &Item::Waiting(id.into())), Some(label));
        }
        for item in [
            Item::Workspace("/project".into()),
            Item::Worktree("/project".into()),
            Item::Shell(1),
            Item::Session("missing".into()),
        ] {
            assert_eq!(row_badge(&tree, &item), None);
        }
    }

    #[test]
    fn an_empty_prompt_passes_ctrl_enter_to_its_session_alone() {
        let session = (Some(Tab::Session("s1".into())), "  \n".to_string());
        assert_eq!(passes_enter(Some(&session)), Some("s1".to_string()));
        let written = (Some(Tab::Session("s1".into())), "hi".to_string());
        assert_eq!(passes_enter(Some(&written)), None);
        let draft = (
            Some(Tab::Draft {
                worktree: "/w".into(),
                role: Role::Worker,
            }),
            String::new(),
        );
        assert_eq!(passes_enter(Some(&draft)), None);
        assert_eq!(passes_enter(Some(&(None, String::new()))), None);
        assert_eq!(passes_enter(None), None);
    }

    #[test]
    fn a_group_folded_shows_its_first_few_again_once_unfolded() {
        let opened = HashSet::from([Group::Files, Group::Links]);
        assert_eq!(
            reopened(&opened, Group::Files, true),
            HashSet::from([Group::Links])
        );
        // Unfolding leaves it as it was.
        assert_eq!(reopened(&opened, Group::Artifacts, false), opened);
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

    fn link(kind: LinkKind, target: &str) -> Link {
        Link {
            kind,
            target: target.into(),
            title: None,
            line: None,
        }
    }

    #[test]
    fn the_overview_heads_each_group_of_links_and_marks_a_file_edited() {
        let rows = [
            link(LinkKind::Artifact, "https://claude.ai/artifact/a"),
            link(LinkKind::Url, "https://a.dev"),
            link(LinkKind::Url, "https://b.dev"),
            link(LinkKind::File(Touch::Edited), "/w/a.rs"),
            link(LinkKind::File(Touch::Read), "/w/b.rs"),
        ];
        let drawn = |folded: &HashSet<Group>| -> Vec<_> {
            link_sections(
                &overview(&[], &rows, folded),
                Path::new("/w"),
                Path::new("/h"),
            )
            .into_iter()
            .map(|s| {
                let rows: Vec<_> = s.rows.iter().map(|r| (r.at, r.icon)).collect();
                (s.group, s.at, s.count, s.folded, rows)
            })
            .collect()
        };
        assert_eq!(
            drawn(&HashSet::new()),
            [
                (Group::Artifacts, 0, 1, false, vec![(1, Icon::AppWindow)]),
                (
                    Group::Links,
                    2,
                    2,
                    false,
                    vec![(3, Icon::Link), (4, Icon::Link)]
                ),
                (
                    Group::Files,
                    5,
                    2,
                    false,
                    vec![(6, Icon::FilePencil), (7, Icon::File)]
                ),
            ]
        );
        assert_eq!(
            drawn(&HashSet::from([Group::Links])),
            [
                (Group::Artifacts, 0, 1, false, vec![(1, Icon::AppWindow)]),
                (Group::Links, 2, 2, true, vec![]),
                (
                    Group::Files,
                    3,
                    2,
                    false,
                    vec![(4, Icon::FilePencil), (5, Icon::File)]
                ),
            ]
        );
    }

    #[test]
    fn an_image_row_shows_its_small_copy_in_place_of_the_icon() {
        let thumb: Arc<str> = "data:image/png;base64,AA==".into();
        let shots = [
            Shot::console(7, 64, 32, 2, Some(thumb.clone())),
            Shot::file("/w/a.heic", Touch::Read, Some(1), None),
        ];
        let none = HashSet::new();
        let drawn = link_sections(
            &with_images(Vec::new(), &shots, &none, &none, 5),
            Path::new("/w"),
            Path::new("/h"),
        );
        assert_eq!(drawn[0].group, Group::Images);
        let rows: Vec<_> = drawn[0]
            .rows
            .iter()
            .map(|r| (r.label.as_str(), r.tag.as_str(), r.file, r.thumb.clone()))
            .collect();
        assert_eq!(
            rows,
            [
                ("Image 64 × 32", "drawn", false, Some(thumb)),
                ("a.heic", "read", true, None),
            ]
        );
        assert_eq!(drawn[0].rows[1].icon, Icon::Photo);
    }

    #[test]
    fn the_overview_lists_drawn_images_of_the_session_shown_after_its_artifacts() {
        let read = (
            "s1".to_string(),
            Links {
                artifacts: vec![link(LinkKind::Artifact, "https://claude.ai/artifact/a")],
                urls: vec![link(LinkKind::Url, "https://a.dev")],
                ..Default::default()
            },
        );
        let mut gallery = Gallery::default();
        let drawn = adroit_term::Drawn {
            width: 1,
            height: 1,
            rgba: vec![255, 0, 0, 255],
        };
        gallery
            .store
            .keep("s1", pictures::Kept::of(drawn, 5).unwrap());
        let open = HashSet::new();
        let Some(Listing::Links(entries)) = overview_listing(
            Some(&read),
            Some(&Tab::Session("s1".into())),
            &open,
            &open,
            &Tree::default(),
            &gallery,
        ) else {
            panic!("no overview");
        };
        let groups: Vec<_> = entries
            .iter()
            .filter_map(|e| match e {
                Entry::Heading { group, .. } => Some(*group),
                _ => None,
            })
            .collect();
        assert_eq!(groups, [Group::Artifacts, Group::Images, Group::Links]);
    }

    #[test]
    fn the_overview_lists_links_only_for_the_session_they_were_read_for() {
        let read = (
            "s1".to_string(),
            Links {
                urls: vec![link(LinkKind::Url, "https://a.dev")],
                ..Default::default()
            },
        );
        let shown = |id: &str| Tab::Session(id.into());
        let open = HashSet::new();
        let t = Tree::default();
        assert!(
            overview_listing(
                Some(&read),
                Some(&shown("s1")),
                &open,
                &open,
                &t,
                &Gallery::default()
            )
            .is_some()
        );
        assert!(
            overview_listing(
                Some(&read),
                Some(&shown("s2")),
                &open,
                &open,
                &t,
                &Gallery::default()
            )
            .is_none()
        );
        assert!(
            overview_listing(
                Some(&read),
                Some(&Tab::Shell(1)),
                &open,
                &open,
                &t,
                &Gallery::default()
            )
            .is_none()
        );
    }

    fn related_to(name: &str, id: Option<&str>, ties: &[Tie]) -> Related {
        Related {
            name: name.into(),
            id: id.map(str::to_string),
            ties: ties.to_vec(),
        }
    }

    #[test]
    fn a_subagent_row_goes_by_its_title_with_its_kind_as_a_badge() {
        use adroit_claude::subagents::{Agent, Standing};
        let agent = |title: Option<&str>, kind: Option<&str>| Agent {
            call: format!("toolu_{}", kind.unwrap_or("none")),
            id: None,
            description: "Persistent hover panel".into(),
            title: title.map(str::to_string),
            kind: kind.map(str::to_string),
            prompt: String::new(),
            standing: Standing::Stopped,
            started_ms: None,
            ended_ms: None,
            steps: Vec::new(),
        };
        let agents = [
            agent(Some("Rename hover to detail"), Some("code-writer")),
            agent(None, None),
        ];
        let none = HashSet::new();
        let drawn = link_sections(
            &with_agents(Vec::new(), &agents, &none),
            Path::new("/w"),
            Path::new("/h"),
        );
        let shown: Vec<_> = drawn[0]
            .rows
            .iter()
            .map(|r| (r.label.as_str(), r.badge.as_deref(), r.tag.as_str()))
            .collect();
        assert_eq!(
            shown,
            [
                ("Rename hover to detail", Some("code-writer"), "stopped"),
                ("Persistent hover panel", None, "stopped"),
            ]
        );
    }

    #[test]
    fn a_session_row_says_how_it_is_tied_and_whether_it_still_runs() {
        let rows = [
            related_to("tests", Some("3"), &[Tie::Started, Tie::Received]),
            related_to("gone", None, &[Tie::Sent]),
        ];
        let drawn = link_sections(
            &overview(&rows, &[], &HashSet::new()),
            Path::new("/w"),
            Path::new("/h"),
        );
        let shown: Vec<_> = drawn[0]
            .rows
            .iter()
            .map(|r| (r.icon, r.label.as_str(), r.tag.as_str()))
            .collect();
        assert_eq!(drawn[0].group, Group::Sessions);
        assert_eq!(
            shown,
            [
                (Icon::MessagePlus, "tests", "started · heard from"),
                (Icon::Message, "gone", "messaged · not running"),
            ]
        );
    }

    #[test]
    fn opening_a_session_row_shows_it_while_it_runs() {
        assert_eq!(
            use_related(&related_to("tests", Some("3"), &[Tie::Started]), Use::Open),
            Went::Show("3".into())
        );
        assert_eq!(
            use_related(&related_to("gone", None, &[Tie::Sent]), Use::Open),
            Went::Told("gone is not running".into())
        );
        assert_eq!(
            use_related(&related_to("tests", Some("3"), &[]), Use::Reveal),
            Went::Nowhere
        );
    }

    #[test]
    fn a_build_that_knows_its_source_names_it_in_the_title() {
        assert_eq!(title(Some("6b5486a-dirty")), "Adroit (6b5486a-dirty)");
        assert_eq!(title(None), "Adroit");
    }

    #[test]
    fn an_unbound_key_in_a_session_s_console_starts_reading_unless_it_is_read_already() {
        // (area, a session shown, its keys passed on, reading) -> reading starts
        let table = [
            (Area::Console, true, false, false, true),
            (Area::Console, true, false, true, false),
            (Area::Console, true, true, false, false),
            (Area::Console, false, false, false, false),
            (Area::Prompt, true, false, false, false),
            (Area::Sidebar, true, false, false, false),
        ];
        for (area, session, passing, reading, want) in table {
            assert_eq!(
                starts_reading(area, session, passing, reading),
                want,
                "{area:?} {session} {passing} {reading}"
            );
        }
    }

    #[test]
    fn a_shell_takes_the_console_s_keys_and_a_waiting_session_the_prompt_s_too() {
        // (area, a session shown, it waits) -> the keys go to the console's program
        let table = [
            (Area::Console, false, false, true),
            (Area::Console, true, false, false),
            (Area::Console, true, true, true),
            (Area::Prompt, false, false, false),
            (Area::Prompt, true, false, false),
            (Area::Prompt, true, true, true),
            (Area::Sidebar, true, true, false),
            (Area::Details, true, true, false),
        ];
        for (area, session, waits, want) in table {
            assert_eq!(
                passes(area, session, waits),
                want,
                "{area:?} {session} {waits}"
            );
        }
    }

    #[test]
    fn text_for_a_session_is_written_in_the_prompt_and_the_rest_goes_to_the_console_s_program() {
        use Typed::*;
        // (area, a session shown, keys passed directly, it waits) -> where text goes
        let table = [
            (Area::Prompt, true, false, false, Prompt),
            (Area::Prompt, false, false, false, Prompt),
            (Area::Prompt, true, false, true, Program),
            (Area::Console, true, false, false, Prompt),
            (Area::Console, true, true, false, Program),
            (Area::Console, true, false, true, Program),
            (Area::Console, false, false, false, Program),
            (Area::Sidebar, true, false, false, Nowhere),
            (Area::Details, false, true, true, Nowhere),
        ];
        for (area, session, direct, waits, want) in table {
            assert_eq!(
                typed_into(area, session, direct, waits),
                want,
                "{area:?} {session} {direct} {waits}"
            );
        }
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
    fn passing_keys_to_a_dialog_of_claude_code_s_says_so_in_the_mode_tip() {
        assert!(mode_tip("PASS", true).contains("dialog"));
        assert!(!mode_tip("PASS", false).contains("dialog"));
    }

    #[test]
    fn tips_that_hold_for_any_agent_name_the_session_rather_than_claude_code() {
        for tip in [
            mode_tip("DIRECT", false),
            row_tip(&Item::Session("s1".into())),
            row_tip(&Item::Waiting("s1".into())),
        ] {
            assert!(!tip.contains("Claude"), "{tip}");
        }
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
