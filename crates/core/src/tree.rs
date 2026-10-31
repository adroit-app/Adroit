//! The tree as the window holds it: the rows built from what the work host
//! lists, which nodes are folded, the row selected, the filter narrowing
//! the rows, and what of the tree the host is asked to keep read.

pub mod nav;

use std::cell::RefCell;
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use adroit_model::files::Entry;
use adroit_model::{EnvironmentId, InboxItem, State, TerminalSession, Tree, inbox};
use adroit_protocol::{Request, Seq, TreeSight, TreeWork, WorkSnapshot};

use crate::chrome::status_line::{self, StatusLine};
use crate::completion::Completions;
use crate::content::{Showing, Tab};
use crate::effect::Effect;
use nav::{Folds, Heading, Item, Place};

#[derive(Debug, Clone, Default, PartialEq)]
pub struct TreeView {
    pub selected: Option<Item>,
    pub folds: Folds,
    /// What the rows are narrowed to.
    pub filter: String,
    /// Whether the filter's field has the keys.
    pub filtering: bool,
    /// Whether every worktree git knows of is listed, those where nothing
    /// runs too.
    pub every_worktree: bool,
    /// Whether the files whose names start with `.` are listed.
    pub show_hidden: bool,
    /// The tree narrowed to what the filter holds.
    pub narrowed: Tree,
    /// The sessions waiting on the user, of the whole tree.
    pub waiting: Vec<InboxItem>,
    /// The sessions waiting on the user that the filter keeps.
    pub narrowed_waiting: Vec<InboxItem>,
    /// The sessions finished that the user has seen since.
    pub completions: Completions,
    /// Every row, top to bottom, the sessions waiting on the user last.
    pub rows: Vec<Item>,
    /// The worktrees whose files the rows list, by their environment.
    marked: BTreeSet<(EnvironmentId, PathBuf)>,
    /// The directories whose entries the rows list, by their environment.
    listed: BTreeSet<(EnvironmentId, PathBuf)>,
    /// What the rows were last built from.
    built_from: Option<(EnvironmentId, Seq, String, Folds, bool)>,
    /// Counts the times the rows were built anew.
    built: u64,
    /// The rows the selection was last kept among.
    kept_among: u64,
    /// The tab the selection last followed.
    followed: Option<Tab>,
    /// The row gone into, whose first row is selected once it is listed,
    /// as a directory's are once the host has read it.
    into: Option<Item>,
    /// The error the status line was last told the sessions cannot be
    /// listed for.
    failing: Option<String>,
}

/// A key on the tree.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Key {
    Down,
    Up,
    First,
    Last,
    /// Fold the row selected when it is open, or else the row it sits under,
    /// going there.
    Fold,
    /// Unfold the row selected and go to the first row under it, once it is
    /// listed, as fern's `l` does on a directory.
    Into,
    /// Show or hide the files whose names start with `.`.
    Hidden,
}

/// Where the content's focused window is, as the header writes it: its
/// workspace, by name and whether it is a scratch one, its worktree, and
/// what the window shows.
#[derive(Debug, Clone, PartialEq)]
pub struct Crumbs {
    pub workspace: Option<(String, bool)>,
    pub worktree: PathBuf,
    pub target: String,
}

/// What the tree's facts are while the host has told none.
fn facts(work: Option<&WorkSnapshot>) -> &TreeWork {
    static NONE: std::sync::LazyLock<TreeWork> = std::sync::LazyLock::new(TreeWork::default);
    work.map_or(&NONE, |w| &w.tree)
}

/// The tree as last listed.
pub fn listed(work: Option<&WorkSnapshot>) -> &Tree {
    facts(work).tree()
}

/// The terminals as last listed.
pub fn terminals(work: Option<&WorkSnapshot>) -> &[TerminalSession] {
    facts(work).terminals()
}

pub(crate) fn select(view: &mut TreeView, row: Option<Item>) -> Vec<Effect> {
    view.selected = row;
    Vec::new()
}

pub(crate) fn fold(view: &mut TreeView, row: &Item, open: bool) -> Vec<Effect> {
    view.folds.set(row, open);
    Vec::new()
}

pub(crate) fn toggle(view: &mut TreeView, row: &Item) -> Vec<Effect> {
    view.folds.toggle(row);
    Vec::new()
}

pub(crate) fn filter(view: &mut TreeView, text: String) -> Vec<Effect> {
    view.filter = text;
    Vec::new()
}

pub(crate) fn filtering(view: &mut TreeView, on: bool) -> Vec<Effect> {
    view.filtering = on;
    Vec::new()
}

/// Every worktree listed, or only those where something runs, looked at
/// afresh.
pub(crate) fn every_worktree(view: &mut TreeView) -> Vec<Effect> {
    view.every_worktree = !view.every_worktree;
    let told = if view.every_worktree {
        "Every worktree is listed, those without sessions too"
    } else {
        "Only the worktrees with sessions are listed"
    };
    vec![Effect::Host(Request::Relist), Effect::Notice(told.into())]
}

pub(crate) fn press(view: &mut TreeView, key: Key) -> Vec<Effect> {
    let rows = &view.rows;
    let current = view.selected.as_ref();
    let next = match key {
        Key::Down => nav::step(rows, current, 1),
        Key::Up => nav::step(rows, current, -1),
        Key::First => rows.first().cloned(),
        Key::Last => rows.last().cloned(),
        Key::Fold => {
            let Some(row) = current.cloned() else {
                return Vec::new();
            };
            if view.folds.is_open(&row) && nav::holds(&row) {
                view.folds.set(&row, false);
                return Vec::new();
            }
            match nav::parent(rows, &view.narrowed, &row) {
                Some(up) => {
                    view.folds.set(&up, false);
                    Some(up)
                }
                None => return Vec::new(),
            }
        }
        Key::Hidden => {
            view.show_hidden = !view.show_hidden;
            return Vec::new();
        }
        Key::Into => {
            if let Some(row) = current.filter(|r| nav::holds(r)) {
                let row = row.clone();
                view.folds.set(&row, true);
                view.into = Some(row);
            }
            return Vec::new();
        }
    };
    view.selected = next;
    Vec::new()
}

/// The rows built anew from the tree the host of `environment` listed, when
/// it, the filter or the folds changed since they were last built. A
/// directory's entries come from what the host listed of it.
pub(crate) fn build(view: &mut TreeView, environment: &EnvironmentId, work: Option<&WorkSnapshot>) {
    let from = work.map(|w| {
        (
            environment.clone(),
            w.seq,
            view.filter.clone(),
            view.folds.clone(),
            view.show_hidden,
        )
    });
    if from.is_some() && view.built_from == from {
        return;
    }
    view.built_from = from;
    let facts = facts(work);
    view.narrowed = nav::narrowed(facts.tree(), &view.filter);
    view.waiting = inbox(facts.tree());
    view.narrowed_waiting = inbox(&view.narrowed);
    let asked = RefCell::new(BTreeSet::new());
    let read = |dir: &Path| {
        asked
            .borrow_mut()
            .insert((environment.clone(), dir.to_path_buf()));
        let mut entries = facts.entries.get(dir).cloned().unwrap_or_default();
        if !view.show_hidden {
            entries.retain(|e| !adroit_model::files::is_hidden(&e.path));
        }
        entries
    };
    let rows = nav::items(
        environment,
        &view.narrowed,
        &view.narrowed_waiting,
        &view.folds,
        &read as &dyn Fn(&Path) -> Vec<Entry>,
    );
    view.marked = rows
        .iter()
        .filter_map(|row| match row {
            Item::Heading(environment, path, Heading::Files) if view.folds.is_open(row) => {
                Some((environment.clone(), path.clone()))
            }
            _ => None,
        })
        .collect();
    view.listed = asked.into_inner();
    if view.rows != rows {
        view.rows = rows;
        view.built += 1;
    }
}

/// The row gone into gives the selection to the first row under it once
/// that is listed, unless the selection has moved off it meanwhile.
pub(crate) fn go_into(view: &mut TreeView) {
    let Some(row) = view.into.clone() else {
        return;
    };
    if view.selected.as_ref() != Some(&row) {
        view.into = None;
        return;
    }
    let rows = &view.rows;
    let first = rows
        .iter()
        .position(|r| *r == row)
        .and_then(|at| rows.get(at + 1))
        .filter(|next| nav::parent(rows, &view.narrowed, next).as_ref() == Some(&row));
    if let Some(first) = first {
        view.selected = Some(first.clone());
        view.into = None;
    }
}

/// A selected row that leaves the rows, as a waiting one does once its
/// session stops waiting, gives way to another rather than to nothing.
/// Only rows built anew are looked at: a row selected ahead of being
/// listed, as a session just started is, stays selected until it is.
pub(crate) fn keep_selection(view: &mut TreeView, showing: &Showing, work: Option<&WorkSnapshot>) {
    if view.kept_among == view.built {
        return;
    }
    view.kept_among = view.built;
    let Some(place) = showing.place.clone() else {
        return;
    };
    let row = nav::row_kept(
        &view.rows,
        listed(work),
        view.selected.as_ref(),
        showing.tab.as_ref(),
        &place,
    );
    if row.is_some() {
        view.selected = row;
    }
}

/// The selection follows the focused window to what it now shows, however
/// it came to show it, unless the selection was moved elsewhere by hand.
pub(crate) fn follow_switch(view: &mut TreeView, showing: &Showing, work: Option<&WorkSnapshot>) {
    let now = showing.tab.clone();
    if now == view.followed {
        return;
    }
    let before = std::mem::replace(&mut view.followed, now.clone());
    let Some(now) = now else {
        return;
    };
    let tree = listed(work);
    // The worktree the window is in follows what it shows.
    let place = place_of_tab(tree, terminals(work), &now).or_else(|| showing.place.clone());
    let Some(place) = place else {
        return;
    };
    let row = nav::row_after_switch(
        &view.rows,
        tree,
        view.selected.as_ref(),
        before.as_ref(),
        &now,
        &place,
    );
    if let Some(row) = row.filter(|row| view.selected.as_ref() != Some(row)) {
        view.selected = Some(row);
    }
}

/// The status line says why the sessions cannot be listed while they
/// cannot, and takes it back once they can.
pub(crate) fn tell_failing(
    view: &mut TreeView,
    status: &mut StatusLine,
    work: Option<&WorkSnapshot>,
) -> Vec<Effect> {
    let now = facts(work).failing.clone();
    if now == view.failing {
        return Vec::new();
    }
    let last = std::mem::replace(&mut view.failing, now.clone());
    status_line::failing(status, last.as_deref(), now.as_deref())
}

/// Typing into the filter stops once the keys leave the sidebar or a menu
/// is opened, so the keys a menu names run what they name; what it narrowed
/// to stays.
pub(crate) fn leave_filter(view: &mut TreeView, left: bool) -> Vec<Effect> {
    if !(left && view.filtering) {
        return Vec::new();
    }
    view.filtering = false;
    vec![Effect::FilterFocus(false)]
}

/// A finished session is seen once the focused window shows it while the
/// user is in the window.
pub(crate) fn see_completions(
    view: &mut TreeView,
    shown: Option<&Tab>,
    focused: bool,
    work: Option<&WorkSnapshot>,
) {
    let shown = match (shown, focused) {
        (Some(Tab::Session(_, id)), true) => Some(id.as_str()),
        _ => None,
    };
    let next = view.completions.updated(listed(work), shown);
    if next != view.completions {
        view.completions = next;
    }
}

/// What the host of `environment` keeps read for the tree.
pub(crate) fn sight(view: &TreeView, environment: &EnvironmentId) -> TreeSight {
    let of = |paths: &BTreeSet<(EnvironmentId, PathBuf)>| {
        paths
            .iter()
            .filter(|(e, _)| e == environment)
            .map(|(_, path)| path.clone())
            .collect()
    };
    TreeSight {
        every_worktree: view.every_worktree,
        marked: of(&view.marked),
        listed: of(&view.listed),
    }
}

/// The worktree `tab`'s thing is in, as the tree lists it.
pub fn place_of_tab(tree: &Tree, terminals: &[TerminalSession], tab: &Tab) -> Option<Place> {
    match tab {
        Tab::Session(_, id) => nav::find(tree, id).map(|(p, _)| p),
        Tab::Terminal(_, id) => terminals
            .iter()
            .find(|s| s.id == *id)
            .map(|s| Place::at(s.cwd.clone())),
        Tab::Draft { worktree, .. } => Some(Place::at(worktree.clone())),
        Tab::Viewer(_, path) => {
            adroit_model::innermost(tree.worktrees().map(|w| &w.worktree.path), path)
                .map(|w| Place::at(w.clone()))
        }
        Tab::Drawn { session, .. } => nav::find(tree, session).map(|(p, _)| p),
    }
}

/// Where the focused window is, while it shows something.
pub fn crumbs(showing: &Showing, work: Option<&WorkSnapshot>) -> Option<Crumbs> {
    let tab = showing.tab.as_ref()?;
    let place = showing.place.as_ref()?;
    let tree = listed(work);
    let workspace = tree
        .workspace_of(&place.worktree)
        .map(|w| (w.name.clone(), w.scratch));
    let target = match tab {
        Tab::Session(_, id) => nav::find(tree, id)
            .map(|(_, s)| s.name.clone())
            .unwrap_or_else(|| id.clone()),
        Tab::Terminal(_, n) => terminals(work)
            .iter()
            .find(|s| s.id == *n)
            .map(|s| s.name.clone())
            .unwrap_or_default(),
        Tab::Draft { .. } => "new session".into(),
        Tab::Viewer(_, path) => path
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default(),
        Tab::Drawn { width, height, .. } => format!("Image {width} × {height}"),
    };
    Some(Crumbs {
        workspace,
        worktree: place.worktree.clone(),
        target,
    })
}

/// How many sessions work now.
pub fn working(work: Option<&WorkSnapshot>) -> usize {
    listed(work)
        .sessions()
        .filter(|(_, s)| s.state == State::Working)
        .count()
}

#[cfg(test)]
mod tests {
    use crate::testing::here;
    use adroit_model::{Session, Wait};
    use adroit_protocol::{Sight, TreeChange};

    use super::*;
    use crate::actions::Action;
    use crate::msg::Msg;
    use crate::scenario::Scenario;
    use crate::testing::{self, listing, patch, session, session_tab, shell, show_in};

    fn rows(run: &Scenario) -> Vec<Item> {
        run.state().tree.rows.clone()
    }

    fn selected(run: &Scenario) -> Option<Item> {
        run.state().tree.selected.clone()
    }

    fn sights(run: &Scenario) -> Vec<TreeSight> {
        run.effects()
            .iter()
            .filter_map(|e| match e {
                Effect::Host(Request::Watch(sight)) => Some(sight.tree.clone()),
                _ => None,
            })
            .collect()
    }

    #[test]
    fn leaving_the_sidebar_or_opening_a_menu_stops_typing_into_the_filter() {
        use crate::content::Area;
        use crate::overlay::context_menu::Target;
        let typing = || {
            Scenario::new()
                .send(Msg::Focus(Area::Sidebar))
                .send(Msg::TreeFiltering(true))
                .send(Msg::TreeFilter("a".into()))
        };
        let run = typing();
        assert!(run.state().tree.filtering);
        assert!(!run.effects().contains(&Effect::FilterFocus(false)));
        let left = [
            Msg::Focus(Area::Console),
            Msg::Focus(Area::Details),
            Msg::MenuOpen {
                x: 0.0,
                y: 0.0,
                target: Target::Row(Item::Session(here(), "a".into())),
            },
        ];
        for msg in left {
            let run = typing().send(msg.clone());
            assert!(!run.state().tree.filtering, "{msg:?}");
            assert_eq!(run.state().tree.filter, "a", "{msg:?}");
            assert!(
                run.last_effects().contains(&Effect::FilterFocus(false)),
                "{msg:?}"
            );
        }
    }

    #[test]
    fn a_finished_session_is_seen_once_shown_while_the_user_is_in_the_window() {
        let done = |id: &str| session(id).cwd("/r").state(State::Done);
        let finished = |run: &Scenario, id: &str| -> State {
            let tree = listed(run.state().host.work.as_ref()).clone();
            let (_, s) = nav::find(&tree, id).unwrap();
            run.state().tree.completions.state(s).clone()
        };
        let listed_done = Scenario::new().event(listing([done("a"), done("b")]));
        assert_eq!(finished(&listed_done, "a"), State::Done);
        let shown = listed_done.send(show_in(session_tab("a"), "/r"));
        assert_eq!(finished(&shown, "a"), State::Idle);
        assert_eq!(finished(&shown, "b"), State::Done);
        // Seen once, it stays seen when another is shown.
        let other = shown.send(show_in(session_tab("b"), "/r"));
        assert_eq!(finished(&other, "a"), State::Idle);
        // Shown while the window is in the back, it is not seen yet.
        let away = Scenario::new()
            .send(Msg::WindowFocused(false))
            .event(listing([done("a")]))
            .send(show_in(session_tab("a"), "/r"));
        assert_eq!(finished(&away, "a"), State::Done);
        let back = away.send(Msg::WindowFocused(true));
        assert_eq!(finished(&back, "a"), State::Idle);
    }

    #[test]
    fn the_rows_are_built_from_what_the_host_lists() {
        let run = Scenario::new().event(listing([
            session("a").cwd("/r"),
            session("b").cwd("/r/w").waiting(Wait::Permission),
        ]));
        let got = rows(&run);
        assert!(got.contains(&Item::Workspace(here(), "/r".into())));
        assert!(got.contains(&Item::Session(here(), "a".into())));
        assert_eq!(got.last(), Some(&Item::Waiting(here(), "b".into())));
        assert_eq!(run.state().tree.waiting.len(), 1);
    }

    #[test]
    fn nothing_is_listed_before_the_host_lists_it() {
        let run = Scenario::new().send(Msg::TreeFilter(String::new()));
        assert!(rows(&run).is_empty());
        assert_eq!(listed(run.state().host.work.as_ref()), &Tree::default());
    }

    #[test]
    fn the_filter_narrows_the_rows_and_the_waiting_ones() {
        let run = Scenario::new()
            .event(listing([
                session("apple").cwd("/r"),
                session("berry").cwd("/r").waiting(Wait::Permission),
            ]))
            .send(Msg::TreeFilter("app".into()));
        let got = rows(&run);
        assert!(got.contains(&Item::Session(here(), "apple".into())));
        assert!(!got.contains(&Item::Session(here(), "berry".into())));
        assert!(run.state().tree.narrowed_waiting.is_empty());
        assert_eq!(run.state().tree.waiting.len(), 1);
    }

    #[test]
    fn a_folded_workspace_hides_its_rows_until_unfolded() {
        let ws = Item::Workspace(here(), "/r".into());
        let run = Scenario::new()
            .event(listing([session("a").cwd("/r")]))
            .send(Msg::TreeFold {
                row: ws.clone(),
                open: false,
            });
        assert_eq!(rows(&run), std::slice::from_ref(&ws));
        let unfolded = run.send(Msg::TreeFoldToggle(ws));
        assert!(rows(&unfolded).contains(&Item::Session(here(), "a".into())));
    }

    #[test]
    fn the_keys_move_the_selection_through_the_rows() {
        let base = || {
            Scenario::new()
                .event(listing([session("a").cwd("/r")]))
                .send(Msg::TreeSelect(Some(Item::Session(here(), "a".into()))))
        };
        let all = rows(&base());
        let first = all.first().cloned();
        let last = all.last().cloned();
        let at = all
            .iter()
            .position(|r| *r == Item::Session(here(), "a".into()))
            .unwrap();
        // (key, row selected after)
        let cases = [
            (Key::First, first.clone()),
            (Key::Last, last),
            (Key::Up, all.get(at - 1).cloned()),
            (Key::Down, all.get((at + 1) % all.len()).cloned()),
        ];
        for (key, want) in cases {
            let run = base().send(Msg::TreeKey(key));
            assert_eq!(selected(&run), want, "{key:?}");
        }
    }

    #[test]
    fn the_fold_key_folds_an_open_node_or_else_the_one_it_sits_under() {
        let heading = Item::Heading(here(), "/r".into(), Heading::Sessions);
        let run = Scenario::new()
            .event(listing([session("a").cwd("/r")]))
            .send(Msg::TreeSelect(Some(Item::Session(here(), "a".into()))))
            .send(Msg::TreeKey(Key::Fold));
        assert_eq!(selected(&run), Some(heading.clone()));
        assert!(!run.state().tree.folds.is_open(&heading));
    }

    /// The worktree `/r` with its Files heading open, listing `/r/src`, a
    /// directory, as the host read it.
    fn with_src() -> (Scenario, Item) {
        let files = Item::Heading(here(), "/r".into(), Heading::Files);
        let run = Scenario::new()
            .event(listing([session("a").cwd("/r")]))
            .send(Msg::TreeFold {
                row: files,
                open: true,
            })
            .event(patch(2).change(TreeChange::Entries {
                dir: "/r".into(),
                entries: vec![Entry {
                    path: "/r/src".into(),
                    dir: true,
                }],
            }));
        let src = Item::File {
            environment: here(),
            worktree: "/r".into(),
            path: "/r/src".into(),
            dir: true,
        };
        (run, src)
    }

    #[test]
    fn going_into_a_directory_unfolds_it_and_selects_its_first_entry_once_read() {
        let (run, src) = with_src();
        let into = run
            .send(Msg::TreeSelect(Some(src.clone())))
            .send(Msg::TreeKey(Key::Into));
        assert!(into.state().tree.folds.is_open(&src));
        assert_eq!(selected(&into), Some(src.clone()));
        let read = into.event(patch(3).change(TreeChange::Entries {
            dir: "/r/src".into(),
            entries: vec![
                Entry {
                    path: "/r/src/lib.rs".into(),
                    dir: false,
                },
                Entry {
                    path: "/r/src/main.rs".into(),
                    dir: false,
                },
            ],
        }));
        let lib = Item::File {
            environment: here(),
            worktree: "/r".into(),
            path: "/r/src/lib.rs".into(),
            dir: false,
        };
        assert_eq!(selected(&read), Some(lib.clone()));
        // Back out, it folds the directory it was in.
        let out = read.send(Msg::TreeKey(Key::Fold));
        assert_eq!(selected(&out), Some(src.clone()));
        assert!(!out.state().tree.folds.is_open(&src));
        // Into it again, its entries are known already.
        let again = out.send(Msg::TreeKey(Key::Into));
        assert_eq!(selected(&again), Some(lib));
    }

    fn pressed(name: &str) -> Msg {
        Msg::Key {
            key: adroit_model::Key::new(name),
            probe: Box::new(crate::console::Probe::default()),
        }
    }

    #[test]
    fn a_file_s_row_takes_the_filer_s_keys_before_the_tree_s() {
        // What `key` resolves to with the row `at` selected and the keys in
        // the tree.
        let on = |at: Option<Item>, key: &str| {
            let (run, src) = with_src();
            let run = run
                .send(Msg::Focus(crate::content::Area::Sidebar))
                .send(Msg::TreeSelect(at.or(Some(src))))
                .send(pressed(key));
            match run.last_effects() {
                [Effect::PerformKey(action, _)] => Some(action.clone()),
                _ => None,
            }
        };
        assert_eq!(on(None, "l"), Some(Action::FilerInto));
        assert_eq!(on(None, "!"), Some(Action::FilerHidden));
        // Off the files, the tree's own `!` lists every worktree.
        let session = Some(Item::Session(here(), "a".into()));
        assert_eq!(on(session, "!"), Some(Action::WorktreesToggle));
    }

    #[test]
    fn hidden_files_are_listed_only_while_shown() {
        let (run, _) = with_src();
        let run = run.event(patch(3).change(TreeChange::Entries {
            dir: "/r".into(),
            entries: vec![
                Entry {
                    path: "/r/.env".into(),
                    dir: false,
                },
                Entry {
                    path: "/r/src".into(),
                    dir: true,
                },
            ],
        }));
        let env = Item::File {
            environment: here(),
            worktree: "/r".into(),
            path: "/r/.env".into(),
            dir: false,
        };
        assert!(!rows(&run).contains(&env));
        let shown = run.send(Msg::TreeKey(Key::Hidden));
        assert!(rows(&shown).contains(&env));
    }

    #[test]
    fn going_into_an_open_node_selects_its_first_row_and_keeps_it_open() {
        let heading = Item::Heading(here(), "/r".into(), Heading::Sessions);
        let run = Scenario::new()
            .event(listing([session("a").cwd("/r")]))
            .send(Msg::TreeSelect(Some(heading.clone())))
            .send(Msg::TreeKey(Key::Into));
        assert!(run.state().tree.folds.is_open(&heading));
        assert_eq!(selected(&run), Some(Item::Session(here(), "a".into())));
    }

    #[test]
    fn moving_away_before_a_directory_is_read_stays_where_it_went() {
        let (run, src) = with_src();
        let moved = run
            .send(Msg::TreeSelect(Some(src)))
            .send(Msg::TreeKey(Key::Into))
            .send(Msg::TreeKey(Key::Up));
        let at = selected(&moved);
        let read = moved.event(patch(3).change(TreeChange::Entries {
            dir: "/r/src".into(),
            entries: vec![Entry {
                path: "/r/src/lib.rs".into(),
                dir: false,
            }],
        }));
        assert_eq!(selected(&read), at);
    }

    #[test]
    fn a_selected_row_that_leaves_gives_way_to_its_other_row() {
        let run = Scenario::new()
            .event(listing([session("a").cwd("/r").waiting(Wait::Permission)]))
            .send(show_in(session_tab("a"), "/r"))
            .send(Msg::TreeSelect(Some(Item::Waiting(here(), "a".into()))))
            .event(patch(2).listed(testing::listed().sessions([session("a").cwd("/r")])));
        assert_eq!(selected(&run), Some(Item::Session(here(), "a".into())));
    }

    #[test]
    fn a_row_selected_ahead_of_being_listed_stays_until_it_is() {
        let run = Scenario::new()
            .event(listing([session("a").cwd("/r")]))
            .send(show_in(session_tab("a"), "/r"))
            .send(Msg::TreeSelect(Some(Item::Session(here(), "new".into()))));
        assert_eq!(selected(&run), Some(Item::Session(here(), "new".into())));
        let listed = run.event(patch(2).listed(
            testing::listed().sessions([session("a").cwd("/r"), session("new").cwd("/r")]),
        ));
        assert_eq!(selected(&listed), Some(Item::Session(here(), "new".into())));
    }

    #[test]
    fn the_selection_follows_the_window_unless_moved_by_hand() {
        let sessions = vec![session("a").cwd("/r"), session("b").cwd("/r")];
        // (selection before the switch, selection after it)
        let cases = [
            (None, Some(Item::Session(here(), "b".into()))),
            (
                Some(Item::Session(here(), "a".into())),
                Some(Item::Session(here(), "b".into())),
            ),
            (
                Some(Item::Workspace(here(), "/r".into())),
                Some(Item::Workspace(here(), "/r".into())),
            ),
        ];
        for (before, after) in cases {
            let run = Scenario::new()
                .event(listing(sessions.clone()))
                .send(show_in(session_tab("a"), "/r"))
                .send(Msg::TreeSelect(before.clone()))
                .send(show_in(session_tab("b"), "/r"));
            assert_eq!(selected(&run), after, "{before:?}");
        }
    }

    #[test]
    fn the_rows_are_the_work_of_the_window_s_environment() {
        let here = EnvironmentId::new("here");
        let run = Scenario::new()
            .send(Msg::Opened {
                home: Default::default(),
                environment: here.clone(),
            })
            .event(listing([session("a").cwd("/r")]));
        assert!(!rows(&run).is_empty());
        assert!(rows(&run).iter().all(|row| row.environment() == &here));
    }

    #[test]
    fn the_host_keeps_read_the_files_the_rows_list() {
        let files = Item::Heading(here(), "/r".into(), Heading::Files);
        let run = Scenario::new()
            .event(listing([session("a").cwd("/r")]))
            .send(Msg::TreeFold {
                row: files.clone(),
                open: true,
            });
        let last = sights(&run).last().cloned().unwrap();
        assert_eq!(last.marked, BTreeSet::from(["/r".into()]));
        assert_eq!(last.listed, BTreeSet::from(["/r".into()]));
        let src = Entry {
            path: "/r/src".into(),
            dir: true,
        };
        let read = run.event(patch(2).change(TreeChange::Entries {
            dir: "/r".into(),
            entries: vec![src],
        }));
        let dir = Item::File {
            environment: here(),
            worktree: "/r".into(),
            path: "/r/src".into(),
            dir: true,
        };
        assert!(rows(&read).contains(&dir));
        let opened = read.send(Msg::TreeFold {
            row: dir,
            open: true,
        });
        let last = sights(&opened).last().cloned().unwrap();
        assert_eq!(last.listed, BTreeSet::from(["/r".into(), "/r/src".into()]));
        let folded = opened.send(Msg::TreeFold {
            row: files,
            open: false,
        });
        assert_eq!(
            sights(&folded).last().cloned().unwrap(),
            TreeSight::default()
        );
    }

    #[test]
    fn listing_every_worktree_is_asked_of_the_host_and_said() {
        let run = Scenario::new().send(Msg::TreeEveryWorktree);
        assert!(run.last_effects().contains(&Effect::Host(Request::Relist)));
        assert!(run.last_effects().contains(&Effect::Notice(
            "Every worktree is listed, those without sessions too".into()
        )));
        assert!(sights(&run).last().is_some_and(|s| s.every_worktree));
        let back = run.send(Msg::TreeEveryWorktree);
        assert!(back.last_effects().contains(&Effect::Notice(
            "Only the worktrees with sessions are listed".into()
        )));
    }

    #[test]
    fn the_status_line_says_why_the_sessions_cannot_be_listed_while_they_cannot() {
        let failing = |error: Option<&str>| TreeChange::Failing {
            error: error.map(Into::into),
        };
        let run = Scenario::new()
            .event(listing(Vec::<Session>::new()))
            .event(patch(2).change(failing(Some("no daemon"))));
        assert_eq!(run.state().chrome.status.line.as_deref(), Some("no daemon"));
        let read = run.event(patch(3).change(failing(None)));
        assert_eq!(read.state().chrome.status.line, None);
    }

    #[test]
    fn the_header_names_where_the_focused_window_is() {
        let run = Scenario::new()
            .event(listing([session("a").cwd("/r")]))
            .send(show_in(session_tab("a"), "/r"));
        let got = crumbs(
            &run.state().content.showing(),
            run.state().host.work.as_ref(),
        );
        assert_eq!(
            got,
            Some(Crumbs {
                workspace: Some(("r".into(), false)),
                worktree: "/r".into(),
                target: "a-name".into(),
            })
        );
        let nothing = run.send(Msg::Window(crate::actions::Action::TabClose));
        assert_eq!(
            crumbs(
                &nothing.state().content.showing(),
                nothing.state().host.work.as_ref()
            ),
            None
        );
    }

    #[test]
    fn a_tab_is_placed_in_the_worktree_its_thing_is_in() {
        let tree = adroit_model::testing::tree()
            .session(session("a").cwd("/r/w"))
            .checkout("/r/w", "/r", "b")
            .build();
        let shell = shell(7, "/r/w/src");
        let place = |w: &str| Some(Place::at(w));
        // (tab, its place)
        let cases = [
            (Tab::Session(here(), "a".into()), place("/r/w")),
            (Tab::Session(here(), "gone".into()), None),
            (Tab::Terminal(here(), 7), place("/r/w/src")),
            (Tab::draft(here(), PathBuf::from("/x")), place("/x")),
            (Tab::Viewer(here(), "/r/w/src/a.rs".into()), place("/r/w")),
            (Tab::Viewer(here(), "/elsewhere/a.rs".into()), None),
            (
                Tab::Drawn {
                    environment: here(),
                    session: "a".into(),
                    id: 1,
                    width: 2,
                    height: 2,
                },
                place("/r/w"),
            ),
        ];
        for (tab, want) in cases {
            assert_eq!(
                place_of_tab(&tree, std::slice::from_ref(&shell), &tab),
                want,
                "{tab:?}"
            );
        }
    }

    #[test]
    fn the_sight_starts_empty() {
        assert_eq!(
            sight(&TreeView::default(), &EnvironmentId::unbound()),
            Sight::default().tree
        );
    }
}
