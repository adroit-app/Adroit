//! What an action does where the keys are now: what a binding runs, and
//! what a click on a row's, a tab's or the menu bar's entry runs.

use adroit_model::Key;

use crate::actions::Action;
use crate::console::Probe;
use crate::console::route;
use crate::content::{self, Area, Axis, Opening, Tab, shows_prompt};
use crate::details::{self, Scroll, Sheet};
use crate::effect::Effect;
use crate::layout::Side;
use crate::msg::Msg;
use crate::overlay::confirm::Ask;
use crate::overlay::rename::Place as RenamePlace;
use crate::state::ViewState;
use crate::tree::nav::{self, Item};
use crate::tree::{self, Key as TreeKey};
use crate::update::step;
use crate::viewers::ViewerAction;
use crate::zoom;

/// Run `action`, which `key` ran, or a click when none, with what the
/// window knows of its programs now. From the sidebar and the details
/// panel an action is on the selected row; from elsewhere, on what the
/// console shows.
pub(crate) fn perform(
    state: &mut ViewState,
    action: Action,
    key: Option<Key>,
    probe: &Probe,
) -> Vec<Effect> {
    let area = state.content.focus;
    let pane = probe.pane && matches!(area, Area::Console | Area::Prompt);
    let prompt_runs = state.console.prompt_runs;
    let cur = state.tree.selected.clone();
    let on_list = matches!(area, Area::Sidebar | Area::Details);
    // In the console, the scroll actions move its view back through what
    // scrolled off instead.
    let on_console = area == Area::Console && pane;
    let half = i32::from((probe.rows / 2).max(1));
    let thing = state.content.active().cloned();
    // The focused window's viewer takes the viewer's actions.
    if area == Area::Console
        && let Some(Tab::Viewer(_, _) | Tab::Drawn { .. }) = &thing
        && let Some(action) = ViewerAction::of(&action)
        && let Some(window) = state.content.windows.focused_window()
    {
        return vec![Effect::Viewer { window, action }];
    }
    match action {
        Action::None => Vec::new(),
        Action::KeysHelp => step(
            state,
            Msg::KeysHelpOpen(route::filing(
                route::help_contexts(area, thing.as_ref()),
                area,
                matches!(cur, Some(Item::File { .. })),
            )),
        ),
        Action::ViewerZoomIn
        | Action::ViewerZoomOut
        | Action::ViewerZoomFit
        | Action::ViewerPageNext
        | Action::ViewerPagePrev
        | Action::ViewerPlay
        | Action::ViewerBack
        | Action::ViewerForward
        | Action::ViewerCopyPath => Vec::new(),
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
        | Action::TabClose => step(state, Msg::Window(action)),
        // The key pressed is Neovim's while the prompt has no agent to send
        // it to.
        Action::ConsoleSend(None)
            if area == Area::Prompt
                && prompt_runs
                && !pane
                && let Some(key) = key.clone() =>
        {
            vec![Effect::PromptKey(key)]
        }
        Action::ConsoleSend(chord) => match chord.map(|c| c.key()).or(key) {
            Some(key) if pane => {
                let mut effects = vec![Effect::PaneKey(key)];
                // Control keys fill the input too: Up brings back an earlier
                // prompt, Ctrl-_ undoes a clear.
                effects.extend(step(state, Msg::ConsoleTyped));
                effects
            }
            _ => Vec::new(),
        },
        Action::ConsoleSendNext => step(state, Msg::ConsoleSendNext),
        Action::ConsolePassEveryKey if pane => step(state, Msg::ConsolePassEveryKey),
        Action::ConsolePassEveryKey => Vec::new(),
        Action::PromptFocus
        | Action::PromptWrite
        | Action::PromptSend
        | Action::PromptImages
        | Action::PromptTakeSuggestion
        | Action::HistoryPrev
        | Action::HistoryNext
            if !shows_prompt(thing.as_ref()) =>
        {
            Vec::new()
        }
        Action::PromptFocus => step(state, Msg::Focus(Area::Prompt)),
        Action::PromptWrite => {
            let mut effects = step(state, Msg::Focus(Area::Prompt));
            // The same key starts writing there, as in Vim.
            if !probe.inserting
                && prompt_runs
                && let Some(key) = key
            {
                effects.push(Effect::PromptKey(key));
            }
            effects
        }
        Action::PromptSend => step(state, Msg::PromptSend),
        Action::PromptTakeSuggestion if prompt_runs => {
            let hands_back = state.console.answers
                && matches!(&thing, Some(Tab::Session(_, id))
                    if nav::find(tree::listed(state.host.work.as_ref()), id)
                        .is_some_and(|(_, s)| s.adroit_editor));
            vec![Effect::TakeSuggestion {
                key: key.unwrap_or_else(|| Key::new("Tab")),
                session: pane,
                hands_back,
                no_images: state.prompt.images.is_empty(),
                tab: thing,
            }]
        }
        Action::PromptTakeSuggestion => Vec::new(),
        Action::PromptImages => step(state, Msg::ImagesChoose),
        Action::HistoryPrev | Action::HistoryNext => {
            let agent = content::agent_of(state, thing.as_ref());
            let back = action == Action::HistoryPrev;
            let mut effects = step(state, Msg::Recall { back, agent });
            effects.extend(step(state, Msg::Focus(Area::Prompt)));
            effects.push(Effect::StartWriting);
            effects
        }
        Action::SessionResume => step(state, Msg::ResumeOpen),
        Action::SessionProfileNext => switch_profile(state, true),
        Action::SessionProfilePrev => switch_profile(state, false),
        Action::SessionStop => step(state, Msg::ConfirmStop { selection: on_list }),
        Action::SessionRename => {
            let chosen = match &cur {
                Some(Item::Session(_, id) | Item::Waiting(_, id)) if on_list => Some(id.clone()),
                _ => match &thing {
                    Some(Tab::Session(_, id)) => Some(id.clone()),
                    _ => None,
                },
            };
            let Some(id) = chosen else {
                return Vec::new();
            };
            let place = if on_list {
                RenamePlace::Sidebar
            } else {
                RenamePlace::Tab
            };
            step(state, Msg::RenameStart { id, place })
        }
        Action::ScrollDown if on_console => vec![Effect::PaneWheel(-1)],
        Action::ScrollUp if on_console => vec![Effect::PaneWheel(1)],
        Action::ScrollHalfDown if on_console => vec![Effect::PaneWheel(-half)],
        Action::ScrollHalfUp if on_console => vec![Effect::PaneWheel(half)],
        Action::ScrollTop if on_console => vec![Effect::PaneWheel(i32::MAX)],
        Action::ScrollBottom if on_console => vec![Effect::PaneWheel(i32::MIN)],
        Action::ScrollDown => vec![Effect::ScrollDetails(Scroll::Down)],
        Action::ScrollUp => vec![Effect::ScrollDetails(Scroll::Up)],
        Action::ScrollHalfDown => vec![Effect::ScrollDetails(Scroll::HalfDown)],
        Action::ScrollHalfUp => vec![Effect::ScrollDetails(Scroll::HalfUp)],
        Action::ScrollTop => vec![Effect::ScrollDetails(Scroll::Top)],
        Action::ScrollBottom => vec![Effect::ScrollDetails(Scroll::Bottom)],
        Action::CursorDown
        | Action::CursorUp
        | Action::CursorFirst
        | Action::CursorLast
        | Action::CursorGo
        | Action::CursorSplit
        | Action::CursorVsplit
        | Action::CursorTab
        | Action::CursorClose
        | Action::CursorShow
        | Action::CursorFold
        | Action::CursorCopy
        | Action::CursorReveal
            if area == Area::Details =>
        {
            // In the details panel, the cursor moves through a list shown
            // and scrolls anything else.
            match details_key(action) {
                Some(key) => step(state, Msg::DetailsKey(key)),
                None => Vec::new(),
            }
        }
        Action::CursorDown => step(state, Msg::TreeKey(TreeKey::Down)),
        Action::CursorUp => step(state, Msg::TreeKey(TreeKey::Up)),
        Action::CursorFirst => step(state, Msg::TreeKey(TreeKey::First)),
        Action::CursorLast => step(state, Msg::TreeKey(TreeKey::Last)),
        Action::CursorGo | Action::CursorSplit | Action::CursorVsplit | Action::CursorTab => {
            let Some(item) = cur else {
                return Vec::new();
            };
            let how = match action {
                Action::CursorSplit => Opening::Split(Axis::Column),
                Action::CursorVsplit => Opening::Split(Axis::Row),
                Action::CursorTab => Opening::Tab,
                _ => Opening::Existing,
            };
            open(state, item, Some(how), true)
        }
        Action::CursorShow => match cur {
            // A row that only folds, or one folded, is gone into: what it
            // holds is what there is to look at.
            Some(item)
                if matches!(
                    nav::opens(
                        &item,
                        &state.tree.narrowed,
                        tree::terminals(state.host.work.as_ref())
                    ),
                    nav::Opens::Fold
                ) || nav::unfolds(&item, &state.tree.folds) =>
            {
                step(state, Msg::TreeKey(TreeKey::Into))
            }
            Some(item) => open(state, item, Some(Opening::Preview), false),
            None => Vec::new(),
        },
        Action::CursorFold => step(state, Msg::TreeKey(TreeKey::Fold)),
        // On the tree's files, the filer's actions are the tree's own.
        Action::FilerDown => perform(state, Action::CursorDown, key, probe),
        Action::FilerUp => perform(state, Action::CursorUp, key, probe),
        Action::FilerInto => perform(state, Action::CursorShow, key, probe),
        Action::FilerOut => perform(state, Action::CursorFold, key, probe),
        Action::FilerOpen => perform(state, Action::CursorGo, key, probe),
        Action::FilerHidden => step(state, Msg::TreeKey(TreeKey::Hidden)),
        Action::CursorClose => step(state, Msg::Window(action)),
        Action::CursorCopy => {
            let terminals = tree::terminals(state.host.work.as_ref());
            let copied = cur
                .as_ref()
                .and_then(|item| nav::copied(item, &state.tree.narrowed, terminals));
            let (text, what) = match copied {
                Some(nav::Copied::Path(p)) => (p.display().to_string(), "path"),
                Some(nav::Copied::Name(n)) => (n, "name"),
                None => return Vec::new(),
            };
            let done = format!("Copied the {what} {text}");
            vec![Effect::Copy { text, done }]
        }
        // Only the overview's files are revealed.
        Action::CursorReveal => Vec::new(),
        Action::WaitingNext | Action::WaitingPrev => {
            let current = match &cur {
                Some(Item::Waiting(_, id) | Item::Session(_, id)) => Some(id.as_str()),
                _ => None,
            };
            let forward = action == Action::WaitingNext;
            let Some(id) = nav::next_waiting(&state.tree.waiting, current, forward) else {
                return Vec::new();
            };
            let waiting = Item::Waiting(state.host.environment.clone(), id);
            let mut effects = open(state, waiting, Some(Opening::Preview), false);
            // It waits on an answer, given in the console.
            effects.extend(step(state, Msg::Focus(Area::Console)));
            effects
        }
        Action::SessionPick => step(state, Msg::PickerOpen),
        Action::SessionScratch => {
            let mut effects = step(state, Msg::PickerClose);
            effects.extend(step(state, Msg::Window(action)));
            effects
        }
        Action::SessionNext
        | Action::SessionPrev
        | Action::WorkspaceNext
        | Action::WorkspacePrev
        | Action::SessionNew
        | Action::ShellOpen
        | Action::NeovimOpen => step(state, Msg::Window(action)),
        // On a shell's row, that shell closes, whichever tab is shown.
        Action::TerminalStop
            if let Some(row @ Item::Terminal(_, id)) = cur.as_ref().filter(|_| on_list) =>
        {
            let id = *id;
            let above = nav::row_above(&state.tree.rows, row);
            let mut effects = step(state, Msg::TreeSelect(above));
            effects.push(Effect::EndShell(id));
            effects
        }
        Action::TerminalStop => match thing {
            Some(Tab::Terminal(_, id)) => vec![Effect::EndShell(id)],
            _ => Vec::new(),
        },
        Action::Refresh => step(state, Msg::Relist),
        Action::DetailsOverview
        | Action::DetailsChanges
        | Action::DetailsLog
        | Action::DetailsAgents
        | Action::DetailsResources => {
            let sheet = match action {
                Action::DetailsChanges => Sheet::Changes,
                Action::DetailsLog => Sheet::Log,
                Action::DetailsAgents => Sheet::Agents,
                Action::DetailsResources => Sheet::Resources,
                _ => Sheet::Overview,
            };
            let mut effects = step(state, Msg::DetailsSheet(sheet));
            effects.extend(step(state, Msg::PanelShow(Side::Details)));
            effects
        }
        Action::WorktreesToggle => step(state, Msg::TreeEveryWorktree),
        Action::SettingsOpen => vec![Effect::OpenSettings],
        Action::DaemonRestart => step(state, Msg::Confirm(Box::new(Ask::RestartDaemon))),
        Action::WindowClose => vec![Effect::CloseWindow],
        Action::WindowOpen => vec![Effect::OpenWindow],
        Action::TreeToggle => step(state, Msg::PanelToggle(Side::Sidebar)),
        Action::DetailsToggle => step(state, Msg::PanelToggle(Side::Details)),
        Action::ZoomIn => step(state, Msg::Zoom(zoom::Step::In)),
        Action::ZoomOut => step(state, Msg::Zoom(zoom::Step::Out)),
        Action::ZoomReset => step(state, Msg::Zoom(zoom::Step::Reset)),
        Action::Filter => {
            let mut effects = step(state, Msg::Focus(Area::Sidebar));
            effects.extend(step(state, Msg::TreeFilter(String::new())));
            effects.push(Effect::FilterFocus(true));
            effects
        }
    }
}

/// The key on the details panel's list `action` presses: the tree's ways of
/// opening a row open its row the same ways.
fn details_key(action: Action) -> Option<details::Key> {
    let key = match action {
        Action::CursorDown => details::Key::Down,
        Action::CursorUp => details::Key::Up,
        Action::CursorFirst => details::Key::First,
        Action::CursorLast => details::Key::Last,
        Action::CursorGo => details::Key::Open,
        Action::CursorShow => details::Key::OpenIn(Opening::Preview),
        Action::CursorSplit => details::Key::OpenIn(Opening::Split(Axis::Column)),
        Action::CursorVsplit => details::Key::OpenIn(Opening::Split(Axis::Row)),
        Action::CursorTab => details::Key::OpenIn(Opening::Tab),
        Action::CursorFold => details::Key::Fold,
        Action::CursorCopy => details::Key::Copy,
        Action::CursorReveal => details::Key::Reveal,
        _ => return None,
    };
    Some(key)
}

/// Open a row where `how` says, or as anything else opens a thing when it
/// says nothing; `take_keys` gives the keys to what opened. Typing stops
/// going to the filter once a row is taken; what it narrowed to stays.
fn open(state: &mut ViewState, item: Item, how: Option<Opening>, take_keys: bool) -> Vec<Effect> {
    let mut effects = Vec::new();
    if state.tree.filtering {
        effects.push(Effect::FilterFocus(false));
    }
    effects.extend(step(
        state,
        Msg::OpenRow {
            item,
            how,
            take_keys,
        },
    ));
    effects
}

/// The profile a new session can start from after the one chosen, or
/// before it, in the order Settings lists them: a session started already
/// keeps its own.
fn switch_profile(state: &mut ViewState, forward: bool) -> Vec<Effect> {
    let tab = state.content.active().cloned();
    if !content::draft::can_choose(tab.as_ref()) {
        return step(
            state,
            Msg::Notice("Only a session not started yet can change its profile".into()),
        );
    }
    let offered = crate::profiles::startable(&state.config.settings);
    let now = content::profile_of(state, tab.as_ref());
    let Some(at) = offered.iter().position(|p| p.id == now) else {
        return Vec::new();
    };
    let len = offered.len();
    let to = if forward {
        (at + 1) % len
    } else {
        (at + len - 1) % len
    };
    let profile = offered[to].id.clone();
    step(state, Msg::DraftProfile { tab, profile })
}

#[cfg(test)]
mod tests;
