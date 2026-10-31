//! What is drawn over the window and holds the keys while it is: a menu
//! opened on a row, a tab or a link, a question to answer, a session's new
//! name being typed, and the banner about the sessions' daemon.

use adroit_model::{EnvironmentId, Key};
use adroit_protocol::Request;

use crate::actions::Action;
use crate::chrome::{self, Chrome, status_line};
use crate::content::{Area, Tab};
use crate::effect::Effect;
use crate::keys_help::{self, Press};
use crate::msg::Msg;
use crate::picker::{self, PickerView};
use crate::state::ViewState;
use crate::tree::nav::Item;
use crate::update::step;

pub mod confirm;
pub mod context_menu;
pub mod rename;
#[cfg(test)]
mod tests;

use confirm::Ask;
use context_menu::Target;
use rename::{Place as RenamePlace, Rename};

#[derive(Debug, Clone, Default, PartialEq)]
pub struct Overlays {
    pub menu: Option<Menu>,
    pub confirm: Option<Ask>,
    pub rename: Option<Rename>,
    pub banner: Banner,
}

/// A menu a right click opened, where it was clicked and on what.
#[derive(Debug, Clone, PartialEq)]
pub struct Menu {
    pub x: f64,
    pub y: f64,
    pub target: Target,
}

/// What the banner says of a daemon the window cannot work with, and
/// whether it was put away.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Banner {
    note: Option<String>,
    closed: bool,
}

impl Banner {
    /// What the banner says while it is shown.
    pub fn shown(&self) -> Option<&str> {
        self.note.as_deref().filter(|_| !self.closed)
    }
}

/// What holds the keys over the window.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Holder {
    Menu,
    KeysHelp,
    Picker,
    Rename,
    Confirm,
}

/// Every overlay open, the one that takes a key first.
pub fn holding(overlays: &Overlays, picker: &PickerView, chrome: &Chrome) -> Vec<Holder> {
    [
        (overlays.menu.is_some(), Holder::Menu),
        (chrome.keys_help.is_some(), Holder::KeysHelp),
        (picker.open.is_some(), Holder::Picker),
        (overlays.rename.is_some(), Holder::Rename),
        (overlays.confirm.is_some(), Holder::Confirm),
    ]
    .into_iter()
    .filter_map(|(open, holder)| open.then_some(holder))
    .collect()
}

/// What takes the keys before anything in the window does.
pub fn holder(state: &ViewState) -> Option<Holder> {
    holding(&state.overlays, &state.picker, &state.chrome)
        .first()
        .copied()
}

/// Whether text typed or pasted goes nowhere while `held` are open: an
/// overlay that types takes it in its own field, and one that does not
/// takes none. A menu left open closes on the next key, and lets text by.
pub fn takes_text_away(held: &[Holder]) -> bool {
    held.iter().any(|h| *h != Holder::Menu)
}

/// Whether the menu bar's entries wait while `held` are open: behind the
/// picker, a question or a name being typed, which hold the keys. A menu or
/// the keys help is put away for them.
pub fn holds_menu_bar(held: &[Holder]) -> bool {
    held.iter()
        .any(|h| matches!(h, Holder::Picker | Holder::Rename | Holder::Confirm))
}

/// A key while an overlay holds the keys.
pub(crate) fn press(state: &mut ViewState, key: Key) -> Vec<Effect> {
    match holder(state) {
        Some(Holder::Menu) => {
            state.overlays.menu = None;
            Vec::new()
        }
        Some(Holder::Picker) => picker::press(state, &key),
        Some(Holder::Confirm) => match key.key.as_str() {
            "y" | "Enter" => answer(state, true),
            "n" | "Escape" => answer(state, false),
            _ => Vec::new(),
        },
        Some(Holder::KeysHelp) => {
            let chrome = &mut state.chrome;
            let here = chrome.keys_help.as_deref().unwrap_or_default();
            match keys_help::press(&chrome.keymap, here, &key) {
                Press::Close => chrome::close_keys_help(chrome),
                Press::Scroll(lines) => vec![Effect::ScrollKeysHelp(lines)],
                Press::Nothing => Vec::new(),
            }
        }
        // Renaming, the keys are its field's; none act behind it.
        Some(Holder::Rename) | None => Vec::new(),
    }
}

pub(crate) fn open_menu(state: &mut ViewState, menu: Menu) -> Vec<Effect> {
    state.overlays.menu = Some(menu);
    Vec::new()
}

pub(crate) fn close_menu(state: &mut ViewState) -> Vec<Effect> {
    state.overlays.menu = None;
    Vec::new()
}

/// Run what a menu entry names on what the menu was opened on, as its key
/// would with that row selected, that tab shown or that link selected.
pub(crate) fn run_menu(state: &mut ViewState, target: Target, action: Action) -> Vec<Effect> {
    state.overlays.menu = None;
    let (at, area) = match target {
        Target::Row(item) => (Msg::TreeSelect(Some(item)), Area::Sidebar),
        Target::Tab(tab) => (Msg::TabReveal(tab), Area::Console),
        Target::Link { at, .. } => (Msg::DetailsSelect(at), Area::Details),
    };
    let mut effects = step(state, at);
    effects.extend(step(state, Msg::Focus(area)));
    effects.push(Effect::Perform(action));
    effects
}

pub(crate) fn ask(state: &mut ViewState, ask: Ask) -> Vec<Effect> {
    state.overlays.confirm = Some(ask);
    Vec::new()
}

/// Ask to end a session: the selected row's, from the tree, or else the
/// one the console shows.
pub(crate) fn ask_stop(state: &mut ViewState, selection: bool) -> Vec<Effect> {
    let chosen = match &state.tree.selected {
        Some(Item::Session(environment, id) | Item::Waiting(environment, id)) if selection => {
            Some((environment, id))
        }
        _ => match state.content.active() {
            Some(Tab::Session(environment, id)) => Some((environment, id)),
            _ => None,
        },
    };
    if let Some((environment, id)) = chosen
        && let Some((_, s)) = state.hosts.session(environment, id)
    {
        state.overlays.confirm = Some(Ask::Stop(environment.clone(), s.clone()));
    }
    Vec::new()
}

/// The question answered: yes asks done what it asked about.
pub(crate) fn answer(state: &mut ViewState, yes: bool) -> Vec<Effect> {
    let Some(ask) = state.overlays.confirm.take() else {
        return Vec::new();
    };
    if !yes {
        return Vec::new();
    }
    vec![match ask {
        Ask::Stop(environment, session) => Effect::Host {
            environment,
            request: Request::Stop {
                session: Box::new(session),
            },
        },
        Ask::KillProcess(environment, session, process) => Effect::Host {
            environment,
            request: Request::Kill {
                session: Box::new(session),
                process,
            },
        },
        Ask::RestartDaemon => Effect::RestartDaemon,
    }]
}

/// Start typing a new name for the session `id` of `environment` where
/// `place` shows it; a session waiting on an answer is answered first.
pub(crate) fn start_rename(
    state: &mut ViewState,
    environment: &EnvironmentId,
    id: &str,
    place: RenamePlace,
) -> Vec<Effect> {
    match state.hosts.session(environment, id).map(|(_, s)| s) {
        Some(s) if s.state.takes_every_key() => {
            let text = format!("Answer {} first, then rename it", s.name);
            status_line::notice(&mut state.chrome.status, text)
        }
        Some(s) => {
            state.overlays.rename = Some(Rename::of(environment, s, place));
            Vec::new()
        }
        None => Vec::new(),
    }
}

/// The name typed so far in the field for `id` of `environment` where
/// `place` shows it.
pub(crate) fn type_name(
    state: &mut ViewState,
    environment: &EnvironmentId,
    id: &str,
    place: RenamePlace,
    text: String,
) -> Vec<Effect> {
    if let Some(r) = state
        .overlays
        .rename
        .as_mut()
        .filter(|r| r.is_for(environment, id, place))
    {
        r.text = text;
    }
    Vec::new()
}

/// The field for `id` of `environment` where `place` shows it left, the
/// name typed kept or given up; an older field leaving takes nothing from a
/// newer one.
pub(crate) fn end_rename(
    state: &mut ViewState,
    environment: &EnvironmentId,
    id: &str,
    place: RenamePlace,
    commit: bool,
) -> Vec<Effect> {
    match Rename::take(&mut state.overlays.rename, environment, id, place) {
        Some(r) if commit => match r.name() {
            Some(name) => {
                let tab = Tab::Session(r.environment.clone(), r.id.clone());
                let agent = crate::content::agent_of(state, Some(&tab));
                vec![Effect::Host {
                    environment: r.environment,
                    request: Request::Rename {
                        id: r.id,
                        name,
                        was: r.was,
                        agent,
                    },
                }]
            }
            None => Vec::new(),
        },
        _ => Vec::new(),
    }
}

/// A name being typed goes when its row or its tab is no longer shown.
pub(crate) fn keep_rename(state: &mut ViewState) {
    let Some(r) = &state.overlays.rename else {
        return;
    };
    let shown = match r.place {
        RenamePlace::Sidebar => state.chrome.panels.layout.show_sidebar
            && state.tree.rows.iter().any(
                |item| matches!(item, Item::Session(e, id) | Item::Waiting(e, id) if *e == r.environment && *id == r.id),
            ),
        RenamePlace::Tab => state
            .content
            .windows
            .tabs()
            .iter()
            .any(|t| matches!(&t.focused().shows, Tab::Session(e, id) if *e == r.environment && *id == r.id)),
    };
    if !shown {
        state.overlays.rename = None;
    }
}

/// How the sessions' daemon stands, as the banner says it; one put away
/// stays away until the daemon is restarted.
pub(crate) fn daemon_standing(state: &mut ViewState, note: Option<String>) -> Vec<Effect> {
    state.overlays.banner.note = note;
    Vec::new()
}

pub(crate) fn daemon_restarted(state: &mut ViewState, note: Option<String>) -> Vec<Effect> {
    state.overlays.banner = Banner {
        note,
        closed: false,
    };
    Vec::new()
}

pub(crate) fn close_banner(state: &mut ViewState) -> Vec<Effect> {
    state.overlays.banner.closed = true;
    Vec::new()
}
