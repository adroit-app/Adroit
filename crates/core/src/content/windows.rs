//! What the keys and clicks do to the content: move the focus among the
//! windows, split, close and arrange them, go through the tabs, and open
//! what a row stands for.

use adroit_model::TerminalProgram;

use super::{
    Area, Axis, Content, Dir, Opening, Shown, Spot, Tab, TerminalAsk, closing, corner, cycled,
    draft, draft_at, moved, settled, shows_prompt, spot_of,
};
use crate::actions::Action;
use crate::details;
use crate::effect::Effect;
use crate::state::ViewState;
use crate::tree::nav::{Item, Place};
use crate::tree::{self, nav};

/// How much of its split a window grows or shrinks by a key.
const RESIZE_STEP: f64 = 0.05;

/// The keys go to `to`, unless it is a side panel that is hidden; a window
/// there is focused first.
fn go(state: &mut ViewState, to: Spot) {
    let layout = state.chrome.panels.layout;
    let hidden = (to.area == Area::Sidebar && !layout.show_sidebar)
        || (to.area == Area::Details && !layout.show_details);
    if hidden {
        return;
    }
    let view = &mut state.content;
    if let Some(w) = to.window
        && view.windows.focused_window() != Some(w)
    {
        view.windows.focus(w);
    }
    view.focus = settled(to.area, shows_prompt(view.active()));
}

/// Run `action` on the content, where the keys are now. From a side panel
/// an action is on the selected row; from elsewhere, on what the focused
/// window shows.
pub(crate) fn act(state: &mut ViewState, action: Action) -> Vec<Effect> {
    let area = state.content.focus;
    let on_list = matches!(area, Area::Sidebar | Area::Details);
    let shown = {
        let l = state.chrome.panels.layout;
        Shown {
            sidebar: l.show_sidebar,
            details: l.show_details,
        }
    };
    let here = spot_of(area, &state.content.windows);
    let prompts_of = |t: Option<&Tab>| shows_prompt(t);
    let view = &mut state.content;
    let thing = view.active().cloned();
    let edit = |view: &mut super::ContentView, f: &dyn Fn(&mut Content<Tab>)| {
        f(&mut view.windows);
        Vec::new()
    };
    match action {
        Action::WindowLeft | Action::WindowDown | Action::WindowUp | Action::WindowRight => {
            let d = match action {
                Action::WindowLeft => Dir::Left,
                Action::WindowDown => Dir::Down,
                Action::WindowUp => Dir::Up,
                _ => Dir::Right,
            };
            // Each spot is found before `go` is told it, which changes the
            // content.
            let to = moved(here, d, &view.windows, prompts_of, shown);
            go(state, to);
            Vec::new()
        }
        Action::WindowNext | Action::WindowPrev => {
            let to = cycled(
                here,
                action == Action::WindowNext,
                &view.windows,
                prompts_of,
                shown,
            );
            go(state, to);
            Vec::new()
        }
        Action::WindowTop | Action::WindowBottom => {
            let to = corner(
                action == Action::WindowTop,
                &view.windows,
                prompts_of,
                shown,
            );
            go(state, to);
            Vec::new()
        }
        Action::WindowBack => {
            if let Some(to) = view.came_from
                && !to.window.is_some_and(|w| view.windows.shows(w).is_none())
            {
                go(state, to);
            }
            Vec::new()
        }
        Action::WindowSplit | Action::WindowVsplit => {
            let axis = if action == Action::WindowVsplit {
                Axis::Row
            } else {
                Axis::Column
            };
            if let Some(t) = thing {
                view.windows.split(axis, t);
            }
            Vec::new()
        }
        Action::WindowNew => {
            let dir = view
                .place
                .as_ref()
                .map_or_else(|| view.scratch.clone(), |p| p.worktree.clone());
            let draft = draft_at(view, state.host.environment.clone(), dir);
            view.windows.split(Axis::Column, draft);
            view.focus = Area::Prompt;
            vec![Effect::StartWriting]
        }
        Action::WindowQuit => closing(view, Content::close_window),
        Action::WindowOnly => closing(view, Content::only),
        Action::TabClose => closing(view, Content::close_tab),
        Action::WindowMoveLeft => edit(view, &|c| c.move_to_edge(Dir::Left)),
        Action::WindowMoveDown => edit(view, &|c| c.move_to_edge(Dir::Down)),
        Action::WindowMoveUp => edit(view, &|c| c.move_to_edge(Dir::Up)),
        Action::WindowMoveRight => edit(view, &|c| c.move_to_edge(Dir::Right)),
        Action::WindowExchange => edit(view, &Content::exchange),
        Action::WindowRotate => edit(view, &Content::rotate),
        Action::WindowEqualize => edit(view, &Content::equalize),
        Action::WindowWider => edit(view, &|c| c.resize(Axis::Row, RESIZE_STEP)),
        Action::WindowNarrower => edit(view, &|c| c.resize(Axis::Row, -RESIZE_STEP)),
        Action::WindowTaller => edit(view, &|c| c.resize(Axis::Column, RESIZE_STEP)),
        Action::WindowShorter => edit(view, &|c| c.resize(Axis::Column, -RESIZE_STEP)),
        Action::WindowMaximizeWidth => edit(view, &|c| c.maximize(Axis::Row)),
        Action::WindowMaximizeHeight => edit(view, &|c| c.maximize(Axis::Column)),
        Action::WindowToTab => edit(view, &Content::to_new_tab),
        Action::TabGo(n) => edit(view, &|c| c.go_to_numbered_tab(n)),
        Action::TabLast => edit(view, &Content::go_to_last_tab),
        // The tabs of the area the keys are in: the details panel's sheets
        // there, the content's tabs elsewhere.
        Action::TabNext | Action::TabPrev => {
            let delta = if action == Action::TabNext { 1 } else { -1 };
            if area == Area::Details {
                return details::step_sheet(&mut state.details, delta);
            }
            edit(view, &|c| c.cycle_tab(delta))
        }
        Action::CursorClose => {
            let Some(item) = state.tree.selected.clone() else {
                return Vec::new();
            };
            let things = nav::stands_for(&item, tree::terminals(state.host.work.as_ref()));
            if things.is_empty() {
                return Vec::new();
            }
            closing(&mut state.content, |c| {
                c.close_showing(|t| things.contains(t))
            })
        }
        Action::SessionNew => match place_here(state, on_list) {
            Some(p) => draft(
                &mut state.content,
                state.host.environment.clone(),
                p.worktree,
                true,
            ),
            None => Vec::new(),
        },
        Action::SessionScratch => {
            let scratch = state.content.scratch.clone();
            draft(
                &mut state.content,
                state.host.environment.clone(),
                scratch,
                true,
            )
        }
        Action::ShellOpen | Action::NeovimOpen => {
            let runs = if action == Action::NeovimOpen {
                TerminalProgram::Neovim
            } else {
                TerminalProgram::Shell
            };
            match place_here(state, on_list) {
                Some(place) => vec![Effect::StartTerminal(TerminalAsk {
                    place,
                    runs,
                    file: None,
                    opening: Opening::Existing,
                    focus: true,
                })],
                None => Vec::new(),
            }
        }
        Action::SessionNext | Action::SessionPrev => {
            let next = nav::next_session(
                tree::listed(state.host.work.as_ref()),
                state.content.active(),
                state.content.place.as_ref(),
                action == Action::SessionNext,
            );
            match next {
                Some(id) => look_at_session(state, id),
                None => Vec::new(),
            }
        }
        Action::WorkspaceNext | Action::WorkspacePrev => {
            let next = nav::next_workspace(
                tree::listed(state.host.work.as_ref()),
                state.content.active(),
                state.content.place.as_ref(),
                &state.content.last_shown,
                action == Action::WorkspaceNext,
            );
            match next {
                Some(id) => look_at_session(state, id),
                None => Vec::new(),
            }
        }
        _ => Vec::new(),
    }
}

/// Where something new starts: the selected row's worktree, from a side
/// panel, or else the one in the middle.
fn place_here(state: &ViewState, on_list: bool) -> Option<Place> {
    state
        .tree
        .selected
        .as_ref()
        .filter(|_| on_list)
        .and_then(|c| nav::place_of(tree::listed(state.host.work.as_ref()), c))
        .or_else(|| state.content.place.clone())
}

/// Open a session wherever it runs, as opening its row does, and select its
/// row, but leave the keys where they are.
pub(crate) fn show_session(state: &mut ViewState, id: String) -> Vec<Effect> {
    show_session_in(state, id, Opening::Existing)
}

/// Look at a session as [`show_session`] opens it, in the preview tab.
fn look_at_session(state: &mut ViewState, id: String) -> Vec<Effect> {
    show_session_in(state, id, Opening::Preview)
}

fn show_session_in(state: &mut ViewState, id: String, how: Opening) -> Vec<Effect> {
    let work = state.host.work.as_ref();
    let Some((p, _)) = nav::find(tree::listed(work), &id) else {
        return Vec::new();
    };
    let tab = Tab::Session(state.host.environment.clone(), id);
    if let Some(row) = nav::row_of(
        &state.tree.rows,
        tree::listed(work),
        tab.environment(),
        Some(&tab),
        &p,
    ) {
        state.tree.selected = Some(row);
    }
    state.content.place = Some(p);
    state.content.windows.open_in(tab, how);
    Vec::new()
}

/// Edit the file at `path`, one that need not be under any worktree, in the
/// Neovim already on it or in one started in its folder, with the keys.
pub(crate) fn edit_file(state: &mut ViewState, path: std::path::PathBuf) -> Vec<Effect> {
    let terminals = tree::terminals(state.host.work.as_ref());
    let folder = path.parent().map(|d| d.to_path_buf()).unwrap_or_default();
    let item = Item::File {
        environment: state.host.environment.clone(),
        worktree: folder.clone(),
        path: path.clone(),
        dir: false,
    };
    let open = nav::stands_for(&item, terminals)
        .into_iter()
        .find(|t| matches!(t, Tab::Terminal(_, _)));
    let content = &mut state.content;
    match open {
        Some(tab) => {
            content.place = Some(Place { worktree: folder });
            content.windows.open(tab);
            content.focus = Area::Console;
            Vec::new()
        }
        None => vec![Effect::StartTerminal(TerminalAsk {
            place: Place { worktree: folder },
            runs: TerminalProgram::Neovim,
            file: Some(path),
            opening: super::Opening::Existing,
            focus: true,
        })],
    }
}

/// Open a row where `how` says, or as anything else opens a thing when it
/// says nothing. Only the keys that open or look rather than split fold a row, and on a
/// folded one they unfold it first. With `take_keys`, the keys go to what
/// opened: the prompt of a draft, or the console.
pub(crate) fn open_row(
    state: &mut ViewState,
    item: Item,
    how: Option<Opening>,
    take_keys: bool,
) -> Vec<Effect> {
    let work = state.host.work.as_ref();
    let terminals = tree::terminals(work);
    let view = &mut state.tree;
    view.selected = Some(item.clone());
    let in_place = matches!(how, None | Some(Opening::Existing | Opening::Preview));
    let unfolds = nav::unfolds(&item, &view.folds);
    if in_place && unfolds {
        view.folds.set(&item, true);
        return Vec::new();
    }
    let opens = nav::opens(&item, &view.narrowed, terminals);
    let toggles = unfolds || matches!(opens, nav::Opens::Fold);
    let content = &mut state.content;
    let show = |content: &mut super::ContentView, thing: Tab| match how {
        Some(how) => content.windows.open_in(thing, how),
        None => content.windows.open(thing),
    };
    let mut effects = Vec::new();
    match opens {
        nav::Opens::Fold if in_place => view.folds.toggle(&item),
        nav::Opens::Fold => {}
        nav::Opens::Shows(thing, p) => {
            if p.is_some() {
                content.place = p;
            }
            show(content, thing);
        }
        nav::Opens::Edit(worktree, file) => effects.push(Effect::StartTerminal(TerminalAsk {
            place: Place { worktree },
            runs: TerminalProgram::Neovim,
            file: Some(file),
            opening: how.unwrap_or(Opening::Existing),
            // A Neovim still starting takes the keys once it has.
            focus: take_keys,
        })),
        nav::Opens::Draft(worktree) => {
            let draft = draft_at(content, item.environment().clone(), worktree.clone());
            show(content, draft);
            content.place = Some(Place { worktree });
        }
    }
    if take_keys && !toggles {
        match content.active() {
            Some(Tab::Draft { .. }) => {
                content.focus = Area::Prompt;
                effects.push(Effect::StartWriting);
            }
            Some(_) => content.focus = Area::Console,
            None => {}
        }
    }
    effects
}

/// The window `id` clicked, which takes the focus.
pub(crate) fn focus_window(state: &mut ViewState, id: super::WindowId) -> Vec<Effect> {
    if state.content.windows.focused_window() != Some(id) {
        state.content.windows.focus(id);
    }
    Vec::new()
}

/// The tab at `at`, counted from 0, shown.
pub(crate) fn show_tab(state: &mut ViewState, at: usize) -> Vec<Effect> {
    state.content.windows.go_to_tab(at);
    Vec::new()
}

/// A window showing `tab` focused, as a menu opened on its tab asks.
pub(crate) fn reveal(state: &mut ViewState, tab: &Tab) -> Vec<Effect> {
    state.content.windows.reveal(tab);
    Vec::new()
}
