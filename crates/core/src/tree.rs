//! The tree as the window holds it: the rows built from what the work host
//! lists, which nodes are folded, the row selected, the filter narrowing
//! the rows, and what of the tree the host is asked to keep read.

pub mod nav;

use std::cell::RefCell;
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use adroit_model::files::Entry;
use adroit_model::{InboxItem, State, TerminalSession, Tree, inbox};
use adroit_protocol::{Request, Seq, TreeSight, TreeWork, WorkSnapshot};

use crate::chrome::status_line::{self, StatusLine};
use crate::content::Tab;
use crate::effect::Effect;
use nav::{Folds, Heading, Item, Place};

/// What the content's focused window shows, and the worktree it is in, as
/// the window tells it.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Showing {
    pub tab: Option<Tab>,
    pub place: Option<Place>,
}

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
    pub showing: Showing,
    /// The tree narrowed to what the filter holds.
    pub narrowed: Tree,
    /// The sessions waiting on the user, of the whole tree.
    pub waiting: Vec<InboxItem>,
    /// The sessions waiting on the user that the filter keeps.
    pub narrowed_waiting: Vec<InboxItem>,
    /// Every row, top to bottom, the sessions waiting on the user last.
    pub rows: Vec<Item>,
    /// The worktrees whose files the rows list.
    marked: BTreeSet<PathBuf>,
    /// The directories whose entries the rows list.
    listed: BTreeSet<PathBuf>,
    /// What the rows were last built from.
    built_from: Option<(Seq, String, Folds)>,
    /// Counts the times the rows were built anew.
    built: u64,
    /// The rows the selection was last kept among.
    kept_among: u64,
    /// The tab the selection last followed.
    followed: Option<Tab>,
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
    /// Fold the row selected when it is open, or go to the row it sits
    /// under, as fern's `h` does.
    Fold,
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

pub(crate) fn showing(view: &mut TreeView, showing: Showing) -> Vec<Effect> {
    view.showing = showing;
    Vec::new()
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
            if view.folds.is_open(&row) && nav::holds(&row, &view.narrowed) {
                view.folds.set(&row, false);
                return Vec::new();
            }
            match nav::parent(rows, &view.narrowed, &row) {
                Some(up) => Some(up),
                None => return Vec::new(),
            }
        }
    };
    view.selected = next;
    Vec::new()
}

/// The rows built anew from the tree the host listed, when it, the filter
/// or the folds changed since they were last built. A directory's entries
/// come from what the host listed of it.
pub(crate) fn build(view: &mut TreeView, work: Option<&WorkSnapshot>) {
    let from = work.map(|w| (w.seq, view.filter.clone(), view.folds.clone()));
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
        asked.borrow_mut().insert(dir.to_path_buf());
        facts.entries.get(dir).cloned().unwrap_or_default()
    };
    let rows = nav::items(
        &view.narrowed,
        &view.narrowed_waiting,
        &view.folds,
        &read as &dyn Fn(&Path) -> Vec<Entry>,
    );
    view.marked = rows
        .iter()
        .filter_map(|row| match row {
            Item::Heading(path, Heading::Files) if view.folds.is_open(row) => Some(path.clone()),
            _ => None,
        })
        .collect();
    view.listed = asked.into_inner();
    if view.rows != rows {
        view.rows = rows;
        view.built += 1;
    }
}

/// The selected row follows its session to the id it goes by now, once it
/// has been taken over.
pub(crate) fn follow_successor(view: &mut TreeView, work: Option<&WorkSnapshot>) {
    let sessions = facts(work).sessions();
    let moved = view
        .selected
        .as_ref()
        .and_then(|row| nav::successor_row(sessions, row));
    if moved.is_some() {
        view.selected = moved;
    }
}

/// A selected row that leaves the rows, as a waiting one does once its
/// session stops waiting, gives way to another rather than to nothing.
/// Only rows built anew are looked at: a row selected ahead of being
/// listed, as a session just started is, stays selected until it is.
pub(crate) fn keep_selection(view: &mut TreeView, work: Option<&WorkSnapshot>) {
    if view.kept_among == view.built {
        return;
    }
    view.kept_among = view.built;
    let Some(place) = view.showing.place.clone() else {
        return;
    };
    let row = nav::row_kept(
        &view.rows,
        listed(work),
        view.selected.as_ref(),
        view.showing.tab.as_ref(),
        &place,
    );
    if row.is_some() {
        view.selected = row;
    }
}

/// The selection follows the focused window to what it now shows, however
/// it came to show it, unless the selection was moved elsewhere by hand.
pub(crate) fn follow_switch(view: &mut TreeView, work: Option<&WorkSnapshot>) {
    let now = view.showing.tab.clone();
    if now == view.followed {
        return;
    }
    let before = std::mem::replace(&mut view.followed, now.clone());
    let Some(now) = now else {
        return;
    };
    let tree = listed(work);
    // The worktree the window is in follows what it shows.
    let place = place_of_tab(tree, terminals(work), &now).or_else(|| view.showing.place.clone());
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

/// What the host keeps read for the tree.
pub(crate) fn sight(view: &TreeView) -> TreeSight {
    TreeSight {
        every_worktree: view.every_worktree,
        marked: view.marked.clone(),
        listed: view.listed.clone(),
    }
}

/// The worktree `tab`'s thing is in, as the tree lists it.
pub fn place_of_tab(tree: &Tree, terminals: &[TerminalSession], tab: &Tab) -> Option<Place> {
    match tab {
        Tab::Session(id) => nav::find(tree, id).map(|(p, _)| p),
        Tab::Terminal(id) => terminals.iter().find(|s| s.id == *id).map(|s| Place {
            worktree: s.cwd.clone(),
        }),
        Tab::Draft { worktree, .. } => Some(Place {
            worktree: worktree.clone(),
        }),
        Tab::Viewer(path) => {
            adroit_model::innermost(tree.worktrees().map(|w| &w.worktree.path), path).map(|w| {
                Place {
                    worktree: w.clone(),
                }
            })
        }
    }
}

/// Where the focused window is, while it shows something.
pub fn crumbs(view: &TreeView, work: Option<&WorkSnapshot>) -> Option<Crumbs> {
    let tab = view.showing.tab.as_ref()?;
    let place = view.showing.place.as_ref()?;
    let tree = listed(work);
    let workspace = tree
        .workspace_of(&place.worktree)
        .map(|w| (w.name.clone(), w.scratch));
    let target = match tab {
        Tab::Session(id) => nav::find(tree, id)
            .map(|(_, s)| s.name.clone())
            .unwrap_or_else(|| id.clone()),
        Tab::Terminal(n) => terminals(work)
            .iter()
            .find(|s| s.id == *n)
            .map(|s| s.name.clone())
            .unwrap_or_default(),
        Tab::Draft { .. } => "new session".into(),
        Tab::Viewer(path) => path
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default(),
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
    use adroit_model::testing::{self, worktree as wt};
    use adroit_model::{Session, Wait, build};
    use adroit_protocol::{Change, Event, Listed, Sight, TreeChange, WorkPatch};

    use super::*;
    use crate::msg::Msg;
    use crate::scenario::Scenario;

    fn session(id: &str, cwd: &str) -> Session {
        Session {
            id: id.into(),
            name: id.into(),
            cwd: cwd.into(),
            ..Default::default()
        }
    }

    fn waiting(id: &str, cwd: &str) -> Session {
        Session {
            state: State::Waiting(Wait::Permission),
            ..session(id, cwd)
        }
    }

    /// The host's first word: the sessions listed, placed in their worktrees.
    fn listing(sessions: Vec<Session>) -> Event {
        Event::Snapshot(Box::new(WorkSnapshot {
            seq: Seq(1),
            tree: TreeWork {
                listed: Some(Box::new(placed(sessions))),
                ..Default::default()
            },
            ..Default::default()
        }))
    }

    fn placed(sessions: Vec<Session>) -> Listed {
        let found = sessions
            .iter()
            .map(|s| (s.cwd.clone(), Some(wt(s.cwd.clone(), "/r", "main"))))
            .collect();
        let tree = build(&sessions, &[], &found, &[], None);
        Listed {
            sessions,
            terminals: Vec::new(),
            tree,
        }
    }

    fn patch(seq: u64, changes: Vec<TreeChange>) -> Event {
        Event::Patch(WorkPatch {
            seq: Seq(seq),
            changes: changes.into_iter().map(Change::Tree).collect(),
        })
    }

    fn shows(tab: Option<Tab>, worktree: Option<&str>) -> Msg {
        Msg::Showing(Box::new(Showing {
            tab,
            place: worktree.map(|w| Place { worktree: w.into() }),
        }))
    }

    fn shows_session(id: &str, worktree: &str) -> Msg {
        shows(Some(Tab::Session(id.into())), Some(worktree))
    }

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
    fn the_rows_are_built_from_what_the_host_lists() {
        let run = Scenario::new().event(listing(vec![session("a", "/r"), waiting("b", "/r/w")]));
        let got = rows(&run);
        assert!(got.contains(&Item::Workspace("/r".into())));
        assert!(got.contains(&Item::Session("a".into())));
        assert_eq!(got.last(), Some(&Item::Waiting("b".into())));
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
            .event(listing(vec![
                session("apple", "/r"),
                waiting("berry", "/r"),
            ]))
            .send(Msg::TreeFilter("app".into()));
        let got = rows(&run);
        assert!(got.contains(&Item::Session("apple".into())));
        assert!(!got.contains(&Item::Session("berry".into())));
        assert!(run.state().tree.narrowed_waiting.is_empty());
        assert_eq!(run.state().tree.waiting.len(), 1);
    }

    #[test]
    fn a_folded_workspace_hides_its_rows_until_unfolded() {
        let ws = Item::Workspace("/r".into());
        let run = Scenario::new()
            .event(listing(vec![session("a", "/r")]))
            .send(Msg::TreeFold {
                row: ws.clone(),
                open: false,
            });
        assert_eq!(rows(&run), std::slice::from_ref(&ws));
        let unfolded = run.send(Msg::TreeFoldToggle(ws));
        assert!(rows(&unfolded).contains(&Item::Session("a".into())));
    }

    #[test]
    fn the_keys_move_the_selection_through_the_rows() {
        let base = || {
            Scenario::new()
                .event(listing(vec![session("a", "/r")]))
                .send(Msg::TreeSelect(Some(Item::Session("a".into()))))
        };
        let all = rows(&base());
        let first = all.first().cloned();
        let last = all.last().cloned();
        let at = all
            .iter()
            .position(|r| *r == Item::Session("a".into()))
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
    fn the_fold_key_folds_an_open_node_and_climbs_from_anything_else() {
        let heading = Item::Heading("/r".into(), Heading::Sessions);
        let run = Scenario::new()
            .event(listing(vec![session("a", "/r")]))
            .send(Msg::TreeSelect(Some(Item::Session("a".into()))))
            .send(Msg::TreeKey(Key::Fold));
        assert_eq!(selected(&run), Some(heading.clone()));
        let folded = run.send(Msg::TreeKey(Key::Fold));
        assert!(!folded.state().tree.folds.is_open(&heading));
        assert_eq!(selected(&folded), Some(heading));
    }

    #[test]
    fn a_selected_row_that_leaves_gives_way_to_its_other_row() {
        let run = Scenario::new()
            .event(listing(vec![waiting("a", "/r")]))
            .send(shows_session("a", "/r"))
            .send(Msg::TreeSelect(Some(Item::Waiting("a".into()))))
            .event(patch(
                2,
                vec![TreeChange::Listed {
                    listed: Box::new(placed(vec![session("a", "/r")])),
                }],
            ));
        assert_eq!(selected(&run), Some(Item::Session("a".into())));
    }

    #[test]
    fn a_row_selected_ahead_of_being_listed_stays_until_it_is() {
        let run = Scenario::new()
            .event(listing(vec![session("a", "/r")]))
            .send(shows_session("a", "/r"))
            .send(Msg::TreeSelect(Some(Item::Session("new".into()))));
        assert_eq!(selected(&run), Some(Item::Session("new".into())));
        let listed = run.event(patch(
            2,
            vec![TreeChange::Listed {
                listed: Box::new(placed(vec![session("a", "/r"), session("new", "/r")])),
            }],
        ));
        assert_eq!(selected(&listed), Some(Item::Session("new".into())));
    }

    #[test]
    fn the_selection_follows_the_window_unless_moved_by_hand() {
        let sessions = vec![session("a", "/r"), session("b", "/r")];
        // (selection before the switch, selection after it)
        let cases = [
            (None, Some(Item::Session("b".into()))),
            (
                Some(Item::Session("a".into())),
                Some(Item::Session("b".into())),
            ),
            (
                Some(Item::Workspace("/r".into())),
                Some(Item::Workspace("/r".into())),
            ),
        ];
        for (before, after) in cases {
            let run = Scenario::new()
                .event(listing(sessions.clone()))
                .send(shows_session("a", "/r"))
                .send(Msg::TreeSelect(before.clone()))
                .send(shows_session("b", "/r"));
            assert_eq!(selected(&run), after, "{before:?}");
        }
    }

    #[test]
    fn the_selection_follows_a_session_taken_over() {
        let mut interactive = session("pid-1", "/r");
        interactive.session_id = "conv".into();
        let mut own = session("own", "/r");
        own.session_id = "pid-1".into();
        let run = Scenario::new()
            .event(listing(vec![interactive]))
            .send(Msg::TreeSelect(Some(Item::Session("pid-1".into()))))
            .event(patch(
                2,
                vec![TreeChange::Listed {
                    listed: Box::new(placed(vec![own])),
                }],
            ));
        assert_eq!(selected(&run), Some(Item::Session("own".into())));
    }

    #[test]
    fn the_host_keeps_read_the_files_the_rows_list() {
        let files = Item::Heading("/r".into(), Heading::Files);
        let run = Scenario::new()
            .event(listing(vec![session("a", "/r")]))
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
        let read = run.event(patch(
            2,
            vec![TreeChange::Entries {
                dir: "/r".into(),
                entries: vec![src],
            }],
        ));
        let dir = Item::File {
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
            .event(listing(Vec::new()))
            .event(patch(2, vec![failing(Some("no daemon"))]));
        assert_eq!(run.state().chrome.status.line.as_deref(), Some("no daemon"));
        let read = run.event(patch(3, vec![failing(None)]));
        assert_eq!(read.state().chrome.status.line, None);
    }

    #[test]
    fn the_header_names_where_the_focused_window_is() {
        let run = Scenario::new()
            .event(listing(vec![session("a", "/r")]))
            .send(shows_session("a", "/r"));
        let got = crumbs(&run.state().tree, run.state().host.work.as_ref());
        assert_eq!(
            got,
            Some(Crumbs {
                workspace: Some(("r".into(), false)),
                worktree: "/r".into(),
                target: "a".into(),
            })
        );
        let nothing = run.send(shows(None, Some("/r")));
        assert_eq!(
            crumbs(&nothing.state().tree, nothing.state().host.work.as_ref()),
            None
        );
    }

    #[test]
    fn a_tab_is_placed_in_the_worktree_its_thing_is_in() {
        let tree = testing::tree()
            .session(testing::session("a").cwd("/r/w"))
            .checkout("/r/w", "/r", "b")
            .build();
        let shell = TerminalSession {
            id: 7,
            cwd: "/r/w/src".into(),
            ..Default::default()
        };
        let place = |w: &str| Some(Place { worktree: w.into() });
        // (tab, its place)
        let cases = [
            (Tab::Session("a".into()), place("/r/w")),
            (Tab::Session("gone".into()), None),
            (Tab::Terminal(7), place("/r/w/src")),
            (Tab::draft(PathBuf::from("/x")), place("/x")),
            (Tab::Viewer("/r/w/src/a.rs".into()), place("/r/w")),
            (Tab::Viewer("/elsewhere/a.rs".into()), None),
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
        assert_eq!(sight(&TreeView::default()), Sight::default().tree);
    }
}
