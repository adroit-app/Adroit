//! What is drawn over the window and holds the keys while it is: a menu
//! opened on a row, a tab or a link, a question to answer, a session's new
//! name being typed, the sheet adding an environment, and the banner about the sessions' daemon.

use adroit_model::{EnvironmentId, Key, Session};
use adroit_protocol::Request;

use crate::actions::Action;
use crate::chrome::{self, Chrome};
use crate::content::{Area, Tab};
use crate::cursor;
use crate::effect::Effect;
use crate::host::Hosts;
use crate::keymap::help::{self, Press};
use crate::keymap::{Context, Keymap, Step};
use crate::msg::Msg;
use crate::picker::{self, PickerView};
use crate::state::ViewState;
use crate::tree::nav::{self, Item};
use crate::update::step;

mod banner;
pub mod confirm;
pub mod context_menu;
pub mod hints;
pub mod lightbox;
pub mod naming;
pub mod relay;
pub mod rename;
#[cfg(test)]
mod tests;

pub use banner::*;
use confirm::Ask;
use context_menu::{Reach, Target};
use lightbox::Lightbox;
use naming::Naming;
pub(crate) use naming::{end_naming, keep_naming, start_naming, type_naming};
pub(crate) use relay::{
    approve_relay, decline_relay, leave_relay, proposed_anew, relay_later, review_relay,
    toggle_relay,
};
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
    pub relays: relay::Relays,
}

/// A menu a right click opened, where it was clicked and on what.
#[derive(Debug, Clone, PartialEq)]
pub struct Menu {
    pub x: f64,
    pub y: f64,
    pub target: Target,
    /// Where the environment of a heading the menu is on works, as the
    /// window knew when it opened.
    pub reach: Reach,
    /// The entry moved to by the keys, none until they move.
    pub cursor: Option<usize>,
}

/// An entry of the menu open, and whether a separator goes above it.
#[derive(Debug, Clone, PartialEq)]
pub struct MenuEntry {
    pub action: Action,
    pub label: &'static str,
    pub apart: bool,
}

/// The entries of `menu` in the order it draws them, for the session it is
/// on as `hosts` list it.
pub fn menu_entries(menu: &Menu, hosts: &Hosts) -> Vec<MenuEntry> {
    let target = match &menu.target {
        Target::Row(row) => Target::Row(row.own().clone()),
        other => other.clone(),
    };
    let session = match &target {
        Target::Row(Item::Session(environment, id) | Item::Waiting(environment, id))
        | Target::Tab(Tab::Session(environment, id)) => {
            let tree = crate::tree::listed(hosts.work(environment));
            nav::find(tree, id).map(|(_, s)| s.clone())
        }
        _ => None,
    };
    context_menu::groups(&target, session.as_ref(), &hosts.environment, menu.reach)
        .into_iter()
        .enumerate()
        .flat_map(|(g, group)| {
            group
                .into_iter()
                .enumerate()
                .map(move |(i, (action, label))| MenuEntry {
                    action,
                    label,
                    apart: g > 0 && i == 0,
                })
        })
        .collect()
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

/// The areas of bindings `holder` reads its keys from, the first first.
pub fn contexts_of(holder: Holder) -> &'static [Context] {
    match holder {
        Holder::Confirm => &[Context::Confirm, Context::Overlay],
        Holder::Lightbox => &[Context::Lightbox],
        Holder::Rename | Holder::Naming => &[],
        Holder::Menu | Holder::KeysHelp | Holder::Picker | Holder::Adding => &[Context::Overlay],
    }
}

/// What `key` runs while `holder` holds the keys, by `keymap`.
pub fn action_over(keymap: &Keymap, holder: Holder, key: &Key) -> Option<Action> {
    match keymap.resolve(contexts_of(holder), &[], key) {
        Step::Run(action) => Some(action),
        Step::Wait | Step::Nothing => None,
    }
}

/// A key while an overlay holds the keys.
pub(crate) fn press(state: &mut ViewState, key: Key) -> Vec<Effect> {
    let Some(holder) = holder(state) else {
        return Vec::new();
    };
    let action = action_over(&state.chrome.keymap, holder, &key);
    match holder {
        Holder::Menu => menu_key(state, action),
        Holder::Picker => picker::press(state, &key),
        Holder::Confirm => match action {
            Some(Action::OverlayChoose) => answer(state, true),
            Some(Action::OverlayClose) => answer(state, false),
            Some(Action::PasteOverwrite) => crate::tree::clipboard::overwrite(state),
            Some(Action::RelayDecline) => decline_relay(state),
            Some(Action::PasteRename) => crate::tree::clipboard::rename(state),
            _ => Vec::new(),
        },
        Holder::KeysHelp => {
            let chrome = &mut state.chrome;
            let here = chrome.keys_help.as_deref().unwrap_or_default();
            match help::press(&chrome.keymap, here, &key) {
                Press::Close => chrome::close_keys_help(chrome),
                Press::Scroll(lines) => vec![Effect::ScrollKeysHelp(lines)],
                Press::Nothing => Vec::new(),
            }
        }
        Holder::Lightbox => match action {
            Some(action) => lightbox::act(state, action),
            None => Vec::new(),
        },
        // The sheet's options are the window's: it moves among them and
        // chooses one itself.
        Holder::Adding => match action {
            Some(Action::OverlayClose) => adding(state, false),
            _ => Vec::new(),
        },
        // Renaming, the keys are its field's; none act behind it.
        Holder::Rename | Holder::Naming => Vec::new(),
    }
}

/// The menu's keys: they move through its entries and run the one moved
/// to; any other key closes it and does nothing else.
fn menu_key(state: &mut ViewState, action: Option<Action>) -> Vec<Effect> {
    let Some(menu) = state.overlays.menu.as_mut() else {
        return Vec::new();
    };
    let len = menu_entries(menu, &state.hosts).len();
    let delta = match action {
        Some(Action::OverlayDown) => 1,
        Some(Action::OverlayUp) => -1,
        Some(Action::OverlayChoose) => {
            let chosen = menu
                .cursor
                .and_then(|at| menu_entries(menu, &state.hosts).into_iter().nth(at));
            return match chosen {
                Some(entry) => {
                    let target = menu.target.clone();
                    run_menu(state, target, entry.action)
                }
                None => Vec::new(),
            };
        }
        _ => return close_menu(state),
    };
    menu.cursor = Some(match menu.cursor {
        Some(at) => cursor::wrap(at, delta, len),
        None if delta > 0 => 0,
        None => len.saturating_sub(1),
    });
    Vec::new()
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
    if yes
        && state
            .overlays
            .confirm
            .as_ref()
            .is_some_and(|a| !a.answerable())
    {
        return Vec::new();
    }
    let Some(ask) = state.overlays.confirm.take() else {
        return Vec::new();
    };
    if !yes {
        leave_relay(state, &ask);
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
        Ask::RestartGateway => vec![Effect::RestartGateway],
        Ask::UpdateMachine(environment, _) => vec![Effect::UpdateMachine(environment)],
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
        Ask::MoveEntries {
            environment,
            worktree,
            dir,
            sources,
        } => vec![Effect::PasteEntries {
            environment,
            worktree,
            entries: crate::tree::clipboard::into(&dir, &sources),
            moving: true,
        }],
        Ask::TrashEntries { environment, paths } => {
            state.tree.marks.clear();
            vec![Effect::TrashEntries { environment, paths }]
        }
        ask @ Ask::Relay { .. } => approve_relay(&ask),
        Ask::Clash(clash) => {
            state.overlays.confirm = Some(Ask::Clash(clash));
            crate::tree::clipboard::overwrite(state)
        }
    }
}
