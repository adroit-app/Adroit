//! What an action does where the keys are now: what a binding runs, and
//! what a click on a row's, a tab's or the menu bar's entry runs.

use adroit_model::Key;
use adroit_protocol::Request;

use crate::actions::Action;
use crate::chrome::panels::layout::Side;
use crate::console::Probe;
use crate::console::route;
use crate::content::{self, Area, Axis, Opening, Tab, shows_prompt};
use crate::details::{self, Scroll};
use crate::effect::Effect;
use crate::msg::Msg;
use crate::overlay::confirm::Ask;
use crate::overlay::naming::Kind as NamingKind;
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
    // Reading the work again reads again the pull request or issue shown.
    if area == Area::Console
        && action == Action::Refresh
        && let Some(tab @ Tab::GitHub { .. }) = &thing
        && let Some(window) = state.content.windows.focused_window()
    {
        let mut effects = vec![Effect::Viewer {
            environment: tab.environment().clone(),
            window,
            action: ViewerAction::Reload,
        }];
        effects.extend(step(state, Msg::Relist));
        return effects;
    }
    // The focused window's viewer takes the viewer's actions.
    if area == Area::Console
        && let Some(tab @ (Tab::Viewer(_, _) | Tab::Drawn { .. } | Tab::GitHub { .. })) = &thing
        && let Some(action) = ViewerAction::of(&action)
        && let Some(window) = state.content.windows.focused_window()
    {
        return vec![Effect::Viewer {
            environment: tab.environment().clone(),
            window,
            action,
        }];
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
        | Action::ViewerCopyPath
        | Action::OverlayClose
        | Action::OverlayDown
        | Action::OverlayUp
        | Action::OverlayChoose => Vec::new(),
        Action::LightboxNext
        | Action::LightboxPrev
        | Action::LightboxZoomIn
        | Action::LightboxZoomOut
        | Action::LightboxZoomFit
        | Action::LightboxPanLeft
        | Action::LightboxPanRight
        | Action::LightboxPanUp
        | Action::LightboxPanDown
        | Action::LightboxClose => step(state, Msg::LightboxAction(action)),
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
        | Action::WindowEdit
        | Action::TabMerge(_)
        | Action::TabMergeLast
        | Action::TabGo(_)
        | Action::TabLast
        | Action::TabMovePrev
        | Action::TabMoveNext
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
        Action::ConsoleRead if pane => {
            let mut effects = match area {
                Area::Prompt => step(state, Msg::Focus(Area::Console)),
                _ => Vec::new(),
            };
            effects.push(Effect::ReadConsole(Vec::new()));
            effects
        }
        Action::ConsoleRead => Vec::new(),
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
                && matches!(&thing, Some(Tab::Session(environment, id))
                    if state.hosts.session(environment, id)
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
        Action::SessionBringBack => {
            let environment = content::focused_environment(state);
            step(state, Msg::InterruptedOpen(environment))
        }
        Action::RelayReview => step(state, Msg::RelayReview),
        Action::RelayDecline => crate::overlay::decline_relay(state),
        Action::SessionProfileNext => switch_profile(state, true),
        Action::SessionProfilePrev => switch_profile(state, false),
        // On a shell's row, that shell closes, whichever tab is shown; ending
        // a session there ends the shell rather than asking of the session
        // shown.
        Action::TerminalStop | Action::SessionStop
            if let Some(row) = cur.as_ref().filter(|_| on_list)
                && let Item::Terminal(environment, id) = row.own() =>
        {
            let end = Effect::EndShell {
                environment: environment.clone(),
                id: *id,
            };
            let above = nav::row_above(state.tree.rows(), row);
            let mut effects = step(state, Msg::TreeSelect(above));
            effects.push(end);
            effects
        }
        Action::SessionStop => step(state, Msg::ConfirmStop { selection: on_list }),
        Action::FileCopy
        | Action::FileMove
        | Action::FilePaste
        | Action::FileTrash
        | Action::FileMark => {
            let Some(row) = cur.filter(|_| on_list) else {
                return Vec::new();
            };
            match action {
                Action::FileCopy => tree::clipboard::hold(state, &row, false),
                Action::FileMove => tree::clipboard::hold(state, &row, true),
                Action::FilePaste => tree::clipboard::paste(&row),
                Action::FileMark => tree::clipboard::mark(state, &row),
                _ => tree::clipboard::trash(state, &row),
            }
        }
        Action::PasteOverwrite => tree::clipboard::overwrite(state),
        Action::PasteRename => tree::clipboard::rename(state),
        Action::FileNew | Action::DirectoryNew => match cur.filter(|_| on_list) {
            Some(row) => {
                let kind = if action == Action::FileNew {
                    NamingKind::File
                } else {
                    NamingKind::Directory
                };
                step(state, Msg::NamingStart { row, kind })
            }
            None => Vec::new(),
        },
        Action::SessionRename => {
            let chosen = match cur.as_ref().map(Item::own) {
                Some(Item::Session(environment, id) | Item::Waiting(environment, id))
                    if on_list =>
                {
                    Some((environment.clone(), id.clone()))
                }
                _ => match &thing {
                    Some(Tab::Session(environment, id)) => Some((environment.clone(), id.clone())),
                    _ => None,
                },
            };
            let Some((environment, id)) = chosen else {
                return Vec::new();
            };
            let place = if on_list {
                RenamePlace::Sidebar
            } else {
                RenamePlace::Tab
            };
            step(
                state,
                Msg::RenameStart {
                    environment,
                    id,
                    place,
                },
            )
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
        Action::CursorFold => step(state, Msg::TreeKey(TreeKey::Fold)),
        // On the tree's files, the filer's actions are the tree's own.
        Action::FilerDown => perform(state, Action::CursorDown, key, probe),
        Action::FilerUp => perform(state, Action::CursorUp, key, probe),
        Action::FilerInto => match cur {
            // A row that only folds, or one folded, is gone into: what it
            // holds is what there is to go to.
            Some(item)
                if matches!(
                    nav::opens(
                        &item,
                        state.tree.narrowed(item.environment()),
                        tree::terminals(state.hosts.work(item.environment())),
                        state.hosts.is_local(item.environment())
                    ),
                    nav::Opens::Fold
                ) || nav::unfolds(&item, &state.tree.folds) =>
            {
                step(state, Msg::TreeKey(TreeKey::Into))
            }
            Some(_) => perform(state, Action::CursorGo, key, probe),
            None => Vec::new(),
        },
        Action::FilerOut => perform(state, Action::CursorFold, key, probe),
        Action::FilerOpen => perform(state, Action::CursorGo, key, probe),
        Action::FilerHidden => step(state, Msg::TreeKey(TreeKey::Hidden)),
        Action::CursorClose => step(state, Msg::Window(action)),
        Action::CursorCopy => {
            let copied = cur.as_ref().and_then(|item| {
                let terminals = tree::terminals(state.hosts.work(item.environment()));
                nav::copied(item, state.tree.narrowed(item.environment()), terminals)
            });
            let (text, what) = match copied {
                Some(nav::Copied::Path(p)) => (p.display().to_string(), "path"),
                Some(nav::Copied::Name(n)) => (n, "name"),
                None => return Vec::new(),
            };
            let done = format!("Copied the {what} {text}");
            vec![Effect::Copy { text, done }]
        }
        Action::CursorBrowse => {
            // In the console, the pull request or issue shown; elsewhere, the
            // row selected.
            let shown = match (area, &thing) {
                (
                    Area::Console,
                    Some(Tab::GitHub {
                        environment,
                        repository,
                        listing,
                        number,
                    }),
                ) => Some((environment.clone(), repository.clone(), *listing, *number)),
                _ => match cur {
                    Some(Item::GitHubItem(environment, root, listing, number)) => {
                        Some((environment, root, listing, number))
                    }
                    _ => None,
                },
            };
            match shown {
                Some((environment, root, listing, number)) => {
                    tree::github_page(state.hosts.work(&environment), &root, listing, number)
                        .map(|target| Effect::Open {
                            environment,
                            target,
                        })
                        .into_iter()
                        .collect()
                }
                None => Vec::new(),
            }
        }
        // Only the overview's files are revealed.
        Action::CursorReveal => Vec::new(),
        Action::WaitingNext | Action::WaitingPrev => {
            let current = match cur.as_ref().map(Item::own) {
                Some(Item::Waiting(environment, id) | Item::Session(environment, id)) => {
                    Some((environment, id.as_str()))
                }
                _ => None,
            };
            let forward = action == Action::WaitingNext;
            let Some((environment, id)) = nav::next_waiting(state.tree.waiting(), current, forward)
            else {
                return Vec::new();
            };
            let waiting = Item::Waiting(environment, id);
            let mut effects = open(state, waiting, Some(Opening::Existing), false);
            // It waits on an answer, given in the console.
            effects.extend(step(state, Msg::Focus(Area::Console)));
            effects
        }
        Action::SessionPick => step(state, Msg::PickerOpen),
        Action::WorkspaceAdd => step(state, Msg::PickerAddOpen),
        // An environment holds no directory to start in: where the session
        // works is picked in that environment.
        Action::SessionNew if on_list && matches!(cur, Some(Item::Environment(_))) => {
            step(state, Msg::PickerOpen)
        }
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
        Action::TerminalStop => match thing {
            Some(Tab::Terminal(environment, id)) => vec![Effect::EndShell { environment, id }],
            _ => Vec::new(),
        },
        Action::Refresh => step(state, Msg::Relist),
        Action::DetailsTab(n) => {
            let mut effects = step(state, Msg::DetailsTab(usize::from(n.saturating_sub(1))));
            effects.extend(step(state, Msg::PanelShow(Side::Details)));
            effects
        }
        Action::WorktreesToggle => step(state, Msg::TreeEveryWorktree),
        Action::SettingsOpen => vec![Effect::OpenSettings(content::focused_environment(state))],
        Action::DaemonRestart => {
            let environment = content::focused_environment(state);
            step(
                state,
                Msg::Confirm(Box::new(Ask::RestartDaemon(environment))),
            )
        }
        Action::GatewayRestart => step(state, Msg::Confirm(Box::new(Ask::RestartGateway))),
        Action::WindowClose => vec![Effect::CloseWindow],
        Action::WindowOpen => vec![Effect::OpenWindow],
        Action::EnvironmentRemove => match cur.as_ref().filter(|_| on_list) {
            Some(Item::Environment(environment)) if *environment != state.hosts.environment => {
                let name = state.hosts.name(environment);
                step(
                    state,
                    Msg::Confirm(Box::new(Ask::RemoveEnvironment(environment.clone(), name))),
                )
            }
            _ => Vec::new(),
        },
        Action::EnvironmentAdd => step(state, Msg::AddingOpen),
        Action::EnvironmentRename => match cur.filter(|_| on_list) {
            Some(Item::Environment(environment)) => step(
                state,
                Msg::RenameStart {
                    environment,
                    id: String::new(),
                    place: RenamePlace::Heading,
                },
            ),
            _ => Vec::new(),
        },
        Action::EnvironmentRetry => {
            vec![Effect::RetryEnvironment(content::focused_environment(
                state,
            ))]
        }
        Action::EnvironmentRestartHost => {
            vec![Effect::RestartWorkHost(content::focused_environment(state))]
        }
        Action::EnvironmentUpdate => {
            let environment = content::focused_environment(state);
            let name = state.hosts.name(&environment);
            step(
                state,
                Msg::Confirm(Box::new(Ask::UpdateMachine(environment, name))),
            )
        }
        Action::EnvironmentPair => vec![Effect::PairAgain(content::focused_environment(state))],
        Action::WorkspaceRemove => match cur.filter(|_| on_list) {
            Some(Item::Workspace(environment, root)) => {
                let tree = tree::listed(state.hosts.work(&environment));
                let Some(ws) = tree.workspaces.iter().find(|ws| ws.root == root) else {
                    return Vec::new();
                };
                let sessions: Vec<_> = ws.sessions().cloned().collect();
                let terminals: Vec<_> = ws
                    .worktrees
                    .iter()
                    .flat_map(|w| w.terminals.iter().map(|t| t.id))
                    .collect();
                // Nothing ends, so there is nothing to ask.
                if sessions.is_empty() && terminals.is_empty() {
                    return vec![Effect::Host {
                        environment,
                        request: Request::DropWorkspace { root },
                    }];
                }
                let ask = Ask::RemoveWorkspace {
                    name: ws.name.clone(),
                    root,
                    sessions,
                    terminals,
                    environment,
                };
                step(state, Msg::Confirm(Box::new(ask)))
            }
            _ => Vec::new(),
        },
        Action::WorktreeClose => match cur.filter(|_| on_list) {
            Some(Item::Worktree(environment, path)) => {
                let tree = tree::listed(state.hosts.work(&environment));
                let Some(w) = tree.worktrees().find(|w| w.worktree.path == path) else {
                    return Vec::new();
                };
                // Nothing runs, so nothing holds the row up.
                if w.sessions.is_empty() && w.terminals.is_empty() {
                    return Vec::new();
                }
                let name = w.worktree.branch.clone().unwrap_or_else(|| {
                    path.file_name().map_or_else(
                        || path.display().to_string(),
                        |f| f.to_string_lossy().into_owned(),
                    )
                });
                let ask = Ask::CloseWorktree {
                    name,
                    sessions: w.sessions.clone(),
                    terminals: w.terminals.iter().map(|t| t.id).collect(),
                    environment,
                };
                step(state, Msg::Confirm(Box::new(ask)))
            }
            _ => Vec::new(),
        },
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
