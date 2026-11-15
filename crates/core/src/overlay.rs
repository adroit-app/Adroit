//! What is drawn over the window and holds the keys while it is: a menu
//! opened on a row, a tab or a link, a question to answer, a session's new
//! name being typed, the sheet adding an environment, and the banner about the sessions' daemon.

use adroit_model::{EnvironmentId, Key, Session};
use adroit_protocol::Request;

use crate::actions::Action;
use crate::chrome::{self, Chrome};
use crate::content::{Area, Tab};
use crate::effect::Effect;
use crate::keymap::help::{self, Press};
use crate::keymap::{Context, Step};
use crate::msg::Msg;
use crate::picker::{self, PickerView};
use crate::state::ViewState;
use crate::tree::nav::Item;
use crate::update::step;

mod banner;
pub mod confirm;
pub mod context_menu;
pub mod lightbox;
pub mod naming;
pub mod rename;
#[cfg(test)]
mod tests;

pub use banner::*;
use confirm::Ask;
use context_menu::Target;
use lightbox::Lightbox;
use naming::Naming;
pub(crate) use naming::{end_naming, keep_naming, start_naming, type_naming};
use rename::Rename;
pub(crate) use rename::{end_rename, keep_rename, start_rename, type_name};

#[derive(Debug, Clone, Default, PartialEq)]
pub struct Overlays {
    pub menu: Option<Menu>,
    pub confirm: Option<Ask>,
    pub rename: Option<Rename>,
    /// A new file or directory being named in the tree's files.
    pub naming: Option<Naming>,
    /// The sheet adding an environment is open.
    pub adding: bool,
    /// A session's images looked at one at a time.
    pub lightbox: Option<Lightbox>,
    pub banner: Banner,
}

/// A menu a right click opened, where it was clicked and on what.
#[derive(Debug, Clone, PartialEq)]
pub struct Menu {
    pub x: f64,
    pub y: f64,
    pub target: Target,
}

/// What holds the keys over the window.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Holder {
    Menu,
    KeysHelp,
    Picker,
    Rename,
    Naming,
    Adding,
    Confirm,
    Lightbox,
}

/// Every overlay open, the one that takes a key first.
pub fn holding(overlays: &Overlays, picker: &PickerView, chrome: &Chrome) -> Vec<Holder> {
    [
        (overlays.menu.is_some(), Holder::Menu),
        (chrome.keys_help.is_some(), Holder::KeysHelp),
        (picker.open.is_some(), Holder::Picker),
        (overlays.rename.is_some(), Holder::Rename),
        (overlays.naming.is_some(), Holder::Naming),
        (overlays.adding, Holder::Adding),
        (overlays.confirm.is_some(), Holder::Confirm),
        (overlays.lightbox.is_some(), Holder::Lightbox),
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
/// picker, a question, a name being typed or the sheet adding an
/// environment, which hold the keys. A menu or
/// the keys help is put away for them.
pub fn holds_menu_bar(held: &[Holder]) -> bool {
    held.iter().any(|h| {
        matches!(
            h,
            Holder::Picker
                | Holder::Rename
                | Holder::Naming
                | Holder::Adding
                | Holder::Confirm
                | Holder::Lightbox
        )
    })
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
            match help::press(&chrome.keymap, here, &key) {
                Press::Close => chrome::close_keys_help(chrome),
                Press::Scroll(lines) => vec![Effect::ScrollKeysHelp(lines)],
                Press::Nothing => Vec::new(),
            }
        }
        Some(Holder::Lightbox) => {
            let keymap = &state.chrome.keymap;
            match keymap.resolve(&[Context::Lightbox], &[], &key) {
                Step::Run(action) => lightbox::act(state, action),
                _ => Vec::new(),
            }
        }
        Some(Holder::Adding) => match key.key.as_str() {
            "Escape" => adding(state, false),
            _ => Vec::new(),
        },
        // Renaming, the keys are its field's; none act behind it.
        Some(Holder::Rename | Holder::Naming) | None => Vec::new(),
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
        Target::Row(item) | Target::Start(item) => (Msg::TreeSelect(Some(item)), Area::Sidebar),
        Target::Tab(tab) => (Msg::TabReveal(tab), Area::Console),
        Target::Link { at, .. } => (Msg::DetailsSelect(at), Area::Details),
    };
    let mut effects = step(state, at);
    effects.extend(step(state, Msg::Focus(area)));
    effects.push(Effect::Perform(action));
    effects
}

pub(crate) fn adding(state: &mut ViewState, open: bool) -> Vec<Effect> {
    state.overlays.adding = open;
    Vec::new()
}

pub(crate) fn ask(state: &mut ViewState, ask: Ask) -> Vec<Effect> {
    state.overlays.confirm = Some(ask);
    Vec::new()
}

/// Ask to end a session: the selected row's, from the tree, or else the
/// one the console shows.
pub(crate) fn ask_stop(state: &mut ViewState, selection: bool) -> Vec<Effect> {
    let chosen = match state.tree.selected.as_ref().map(Item::own) {
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
    let stop = |environment: &EnvironmentId, session: Session| Effect::Host {
        environment: environment.clone(),
        request: Request::Stop {
            session: Box::new(session),
        },
    };
    match ask {
        Ask::Stop(environment, session) => vec![stop(&environment, session)],
        Ask::KillProcess(environment, session, process) => vec![Effect::Host {
            environment,
            request: Request::Kill {
                session: Box::new(session),
                process,
            },
        }],
        Ask::RestartDaemon(environment) => vec![Effect::RestartDaemon(environment)],
        Ask::RemoveEnvironment(environment, _) => vec![Effect::ArchiveEnvironment(environment)],
        Ask::RemoveWorkspace {
            environment,
            root,
            sessions,
            terminals,
            ..
        } => {
            let ends = terminals.iter().map(|id| Effect::EndShell {
                environment: environment.clone(),
                id: *id,
            });
            let dropped = Effect::Host {
                environment: environment.clone(),
                request: Request::DropWorkspace { root },
            };
            sessions
                .into_iter()
                .map(|s| stop(&environment, s))
                .chain(ends)
                .chain([dropped])
                .collect()
        }
        Ask::CloseWorktree {
            environment,
            sessions,
            terminals,
            ..
        } => {
            let ends = terminals.iter().map(|id| Effect::EndShell {
                environment: environment.clone(),
                id: *id,
            });
            sessions
                .into_iter()
                .map(|s| stop(&environment, s))
                .chain(ends)
                .collect()
        }
    }
}
