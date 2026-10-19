//! The sidebar as one column of items: the inbox first, then the workspaces.

use std::collections::HashSet;
use std::path::{Path, PathBuf};

use adroit_model::{InboxItem, Session, State, Tree, WorktreeNode};

use crate::tabs::Tab;

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Item {
    /// A session waiting on the user, listed in the inbox.
    Waiting(String),
    /// A workspace, by the repository or directory it is.
    Workspace(PathBuf),
    Worktree(PathBuf),
    Session(String),
    /// A shell adroit runs, by the daemon's id.
    Shell(u128),
}

impl Item {
    /// The key used to remember whether a node is collapsed.
    pub fn fold_key(&self) -> Option<String> {
        match self {
            Item::Workspace(root) => Some(format!("ws:{}", root.display())),
            Item::Worktree(path) => Some(format!("wt:{}", path.display())),
            _ => None,
        }
    }
}

/// Every item the sidebar shows, top to bottom. Children of a collapsed node
/// are left out.
pub fn items(tree: &Tree, inbox: &[InboxItem], collapsed: &HashSet<String>) -> Vec<Item> {
    let open = |i: &Item| !i.fold_key().is_some_and(|k| collapsed.contains(&k));
    let mut out: Vec<Item> = inbox
        .iter()
        .map(|i| Item::Waiting(i.session.id.clone()))
        .collect();
    for ws in &tree.workspaces {
        let wi = Item::Workspace(ws.root.clone());
        let ws_open = open(&wi);
        out.push(wi);
        if !ws_open {
            continue;
        }
        if ws.flat() {
            out.extend(ws.worktrees.iter().flat_map(rows_in));
            continue;
        }
        for w in &ws.worktrees {
            let ti = Item::Worktree(w.worktree.path.clone());
            let t_open = open(&ti);
            out.push(ti);
            if t_open {
                out.extend(rows_in(w));
            }
        }
    }
    out
}

/// The rows of what runs in a worktree: its sessions, then its shells.
fn rows_in(w: &WorktreeNode) -> impl Iterator<Item = Item> + '_ {
    let sessions = w.sessions.iter().map(|s| Item::Session(s.id.clone()));
    sessions.chain(w.shells.iter().map(|s| Item::Shell(s.id)))
}

/// The row above `item` in `list`, which stands where it stood once it is
/// gone: a shell's is always of its own worktree. `None` for the first row
/// and for one not listed.
pub fn row_above(list: &[Item], item: &Item) -> Option<Item> {
    let at = list.iter().position(|i| i == item)?;
    at.checked_sub(1).map(|i| list[i].clone())
}

/// The item `delta` steps from `current`, clamped to the ends. With nothing
/// selected the first item is chosen.
pub fn step(items: &[Item], current: Option<&Item>, delta: isize) -> Option<Item> {
    if items.is_empty() {
        return None;
    }
    let at = current.and_then(|c| items.iter().position(|i| i == c));
    let next = match at {
        Some(i) => (i as isize + delta).clamp(0, items.len() as isize - 1) as usize,
        None => 0,
    };
    Some(items[next].clone())
}

/// The index `delta` places from `at` in a list of `len`, wrapping around.
pub fn wrap(at: usize, delta: isize, len: usize) -> usize {
    (at as isize + delta).rem_euclid(len as isize) as usize
}

/// The next (or previous) waiting session after `current`, wrapping around.
pub fn next_waiting(inbox: &[InboxItem], current: Option<&str>, forward: bool) -> Option<String> {
    if inbox.is_empty() {
        return None;
    }
    let n = inbox.len();
    let at = current.and_then(|c| inbox.iter().position(|i| i.session.id == c));
    let i = match (at, forward) {
        (Some(i), true) => (i + 1) % n,
        (Some(i), false) => (i + n - 1) % n,
        (None, true) => 0,
        (None, false) => n - 1,
    };
    Some(inbox[i].session.id.clone())
}

/// The session after (or before) the one `shown`, over every session in the
/// sidebar's order, folded away or not, wrapping around. A tab that shows no
/// session stands after the sessions of its worktree, `place`; with nothing
/// shown, the first session is next and the last one before.
pub fn next_session(
    tree: &Tree,
    shown: Option<&Tab>,
    place: Option<&Place>,
    forward: bool,
) -> Option<String> {
    let all: Vec<(usize, &Session)> = tree
        .worktrees()
        .enumerate()
        .flat_map(|(w, node)| node.sessions.iter().map(move |s| (w, s)))
        .collect();
    if all.is_empty() {
        return None;
    }
    let n = all.len();
    let at = match shown {
        Some(Tab::Session(id)) => all.iter().position(|(_, s)| &s.id == id),
        _ => None,
    };
    let i = match at {
        Some(i) if forward => i + 1,
        Some(i) => i + n - 1,
        None => {
            let worktree =
                place.and_then(|p| tree.worktrees().position(|w| w.worktree.path == p.worktree));
            let after = worktree.map_or(0, |wt| all.iter().filter(|(w, _)| *w <= wt).count());
            if forward { after } else { after + n - 1 }
        }
    };
    Some(all[i % n].1.id.clone())
}

/// Where the middle of the window is: the worktree, or the directory outside
/// git, its tabs are of.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Place {
    pub worktree: PathBuf,
}

/// Where a prompt sent with nothing open starts its session: the worktree
/// last opened, or else the one place sessions run; `scratch`, the scratch
/// folder, when it could mean more than one place or none.
pub fn start_place(tree: &Tree, last: Option<&Place>, scratch: &Path) -> Place {
    if let Some(p) = last {
        return p.clone();
    }
    // A worktree listed for its shells alone, or only because every worktree
    // is shown, runs no session.
    let mut all = tree.worktrees().filter(|w| !w.sessions.is_empty());
    match (all.next(), all.next()) {
        (Some(w), None) => Place {
            worktree: w.worktree.path.clone(),
        },
        _ => Place {
            worktree: scratch.to_path_buf(),
        },
    }
}

/// Find a session by id, with the place it opens.
pub fn find<'a>(tree: &'a Tree, id: &str) -> Option<(Place, &'a Session)> {
    tree.find(id).map(|(_, wt, s)| {
        (
            Place {
                worktree: wt.worktree.path.clone(),
            },
            s,
        )
    })
}

/// The id a session listed as `id` before goes by now, when it has been
/// taken over and runs in adroit under an id of its own. `None` while it is still
/// listed as `id`, and once it is gone.
pub fn successor<'a>(sessions: &'a [Session], id: &str) -> Option<&'a str> {
    if sessions.iter().any(|s| s.id == id) {
        return None;
    }
    sessions
        .iter()
        .find(|s| s.session_id == id)
        .map(|s| s.id.as_str())
}

/// Whether the session `id` names is no longer listed, nor goes by another
/// id now.
pub fn is_gone(sessions: &[Session], id: &str) -> bool {
    !sessions.iter().any(|s| s.id == id) && successor(sessions, id).is_none()
}

/// Tabs kept for what they show, with each session's under the id it goes
/// by now, and those of sessions no longer listed left out.
pub fn followed(sessions: &[Session], tabs: &HashSet<Tab>) -> HashSet<Tab> {
    tabs.iter()
        .filter_map(|tab| match tab {
            Tab::Session(id) if sessions.iter().any(|s| &s.id == id) => Some(tab.clone()),
            Tab::Session(id) => successor(sessions, id).map(|n| Tab::Session(n.to_string())),
            _ => Some(tab.clone()),
        })
        .collect()
}

/// The row to select in place of `item` when the session it is for has moved
/// over to adroit: that session's row under its new id, in the inbox while
/// an inbox row is selected and the session still waits.
pub fn successor_row(sessions: &[Session], item: &Item) -> Option<Item> {
    let (Item::Session(id) | Item::Waiting(id)) = item else {
        return None;
    };
    let new = successor(sessions, id)?;
    let waits = sessions
        .iter()
        .any(|s| s.id == new && matches!(s.state, State::Waiting(_)));
    Some(if matches!(item, Item::Waiting(_)) && waits {
        Item::Waiting(new.to_string())
    } else {
        Item::Session(new.to_string())
    })
}

/// The row to select once `selected` is no longer in `list`: the same
/// session's other row, as when it stops waiting, or else the row of the tab
/// `shown` in `place`. `None` while `selected` is still listed.
pub fn row_kept(
    list: &[Item],
    tree: &Tree,
    selected: Option<&Item>,
    shown: Option<&Tab>,
    place: &Place,
) -> Option<Item> {
    let selected = selected?;
    if list.contains(selected) {
        return None;
    }
    let same = match selected {
        Item::Waiting(id) => Some(Item::Session(id.clone())),
        Item::Session(id) => Some(Item::Waiting(id.clone())),
        _ => None,
    };
    if let Some(row) = same.filter(|row| list.contains(row)) {
        return Some(row);
    }
    row_of(list, tree, shown, place)
}

/// `tree` with only the sessions and shells whose names hold `text`, any
/// case, and the worktrees and workspaces that still hold one; a workspace or a worktree
/// whose own name holds it keeps everything beneath it. An empty `text` keeps
/// everything.
pub fn narrowed(tree: &Tree, text: &str) -> Tree {
    if text.is_empty() {
        return tree.clone();
    }
    let want = text.to_lowercase();
    let named = |name: &str| name.to_lowercase().contains(&want);
    let mut out = tree.clone();
    for ws in &mut out.workspaces {
        if named(&ws.name) {
            continue;
        }
        ws.worktrees.retain_mut(|w| {
            let own = w.worktree.branch.as_deref().is_some_and(named)
                || w.worktree
                    .path
                    .file_name()
                    .is_some_and(|f| named(&f.to_string_lossy()));
            if !own {
                w.sessions.retain(|s| named(&s.name));
                w.shells.retain(|s| named(&s.name));
            }
            own || !w.sessions.is_empty() || !w.shells.is_empty()
        });
    }
    out.workspaces
        .retain(|ws| named(&ws.name) || !ws.worktrees.is_empty());
    out
}

/// The tab to show once `closing` is closed among `tabs`: the one before it,
/// or else the one after it.
pub fn tab_after(tabs: &[Tab], closing: &Tab) -> Option<Tab> {
    let at = tabs.iter().position(|t| t == closing)?;
    at.checked_sub(1)
        .or_else(|| (at + 1 < tabs.len()).then_some(at + 1))
        .map(|i| tabs[i].clone())
}

/// What the console shows once a tab of `place` has closed: its first
/// session, or else the first of `shells`, the shells left in the worktree.
pub fn tab_after_closing(tree: &Tree, place: &Place, shells: &[u128]) -> Option<Tab> {
    sessions_in(tree, place)
        .first()
        .map(|s| Tab::Session(s.id.clone()))
        .or_else(|| shells.first().map(|&id| Tab::Shell(id)))
}

/// The row in `list` for what a tab shows: the session's or the shell's own
/// row, or else the nearest row above it still shown, its worktree or
/// workspace. `None` when none is listed.
pub fn row_of(list: &[Item], tree: &Tree, shown: Option<&Tab>, place: &Place) -> Option<Item> {
    let mut rows: Vec<Item> = match shown {
        Some(Tab::Session(id)) => vec![Item::Session(id.clone())],
        Some(Tab::Shell(id)) => vec![Item::Shell(*id)],
        _ => Vec::new(),
    };
    rows.push(Item::Worktree(place.worktree.clone()));
    if let Some(ws) = tree.workspace_of(&place.worktree) {
        rows.push(Item::Workspace(ws.root.clone()));
    }
    rows.into_iter().find(|item| list.contains(item))
}

/// The row to select once the console shows `now` instead of `before`, both in
/// `place`: the new tab's, when the selection was on the old tab's row, on a
/// row no longer listed, or on nothing. `None` leaves the selection where it was moved by hand.
pub fn row_after_switch(
    list: &[Item],
    tree: &Tree,
    selected: Option<&Item>,
    before: Option<&Tab>,
    now: &Tab,
    place: &Place,
) -> Option<Item> {
    let on_old = match (selected, before) {
        (None, _) => true,
        (Some(_), None) => false,
        (Some(item), Some(tab)) => {
            !list.contains(item)
                || matches!((item, tab), (Item::Waiting(w), Tab::Session(o)) if w == o)
                || Some(item) == row_of(list, tree, Some(tab), place).as_ref()
        }
    };
    on_old
        .then(|| row_of(list, tree, Some(now), place))
        .flatten()
}

/// The worktree or directory an item works in: a session's own, the one a
/// shell was started in, a worktree itself, a workspace's main checkout or
/// directory.
pub fn place_of(tree: &Tree, item: &Item) -> Option<Place> {
    match item {
        Item::Waiting(id) | Item::Session(id) => find(tree, id).map(|(p, _)| p),
        Item::Shell(id) => tree.shell(*id).map(|(_, _, s)| Place {
            worktree: s.worktree.clone(),
        }),
        Item::Worktree(path) => Some(Place {
            worktree: path.clone(),
        }),
        // Not the root itself: a bare repository, a submodule or a separate
        // git directory is named by its git directory.
        Item::Workspace(root) => {
            let ws = tree.workspaces.iter().find(|ws| &ws.root == root)?;
            let main = ws.worktrees.iter().find(|w| w.worktree.is_main());
            main.or(ws.worktrees.first()).map(|w| Place {
                worktree: w.worktree.path.clone(),
            })
        }
    }
}

/// The sessions running in a worktree, for the tabs above the console.
pub fn sessions_in<'a>(tree: &'a Tree, place: &Place) -> Vec<&'a Session> {
    tree.worktrees()
        .filter(|w| w.worktree.path == place.worktree)
        .flat_map(|w| w.sessions.iter())
        .collect()
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use super::*;
    use adroit_model::{Kind, Shell, State, Wait, Worktree, build, inbox};

    fn s(id: &str, cwd: &str, state: State) -> Session {
        Session {
            id: id.into(),
            name: id.into(),
            cwd: cwd.into(),
            kind: Kind::Own,
            state,
            ..Default::default()
        }
    }

    fn wt(path: &str, repo: &str, branch: &str) -> Worktree {
        Worktree {
            path: path.into(),
            repo: repo.into(),
            branch: Some(branch.into()),
        }
    }

    const OAUTH: &str = "/api/.claude/worktrees/oauth";

    /// `api` is a repository with sessions in its main checkout and in a
    /// worktree; `web` one with sessions only in its main checkout; `tmp` is
    /// outside git.
    fn tree() -> Tree {
        let found: HashMap<PathBuf, Option<Worktree>> = [
            ("/api", Some(wt("/api", "/api", "main"))),
            (OAUTH, Some(wt(OAUTH, "/api", "oauth"))),
            ("/web", Some(wt("/web", "/web", "main"))),
            ("/tmp", None),
        ]
        .into_iter()
        .map(|(p, w)| (PathBuf::from(p), w))
        .collect();
        build(
            &[
                s("a", "/api", State::Working),
                s("b", "/api", State::Waiting(Wait::Permission)),
                s("o", OAUTH, State::Idle),
                s("c", "/web", State::Idle),
                s("z", "/tmp", State::Waiting(Wait::Input)),
            ],
            &[],
            &found,
            &[],
            None,
        )
    }

    fn ws(root: &str) -> Item {
        Item::Workspace(root.into())
    }

    fn worktree(path: &str) -> Item {
        Item::Worktree(path.into())
    }

    fn session(id: &str) -> Item {
        Item::Session(id.into())
    }

    fn at(path: &str) -> Place {
        Place {
            worktree: path.into(),
        }
    }

    #[test]
    fn the_inbox_comes_first_then_each_workspace_with_its_worktrees_and_sessions() {
        let t = tree();
        let got = items(&t, &inbox(&t), &HashSet::new());
        assert_eq!(
            got,
            vec![
                Item::Waiting("b".into()),
                Item::Waiting("z".into()),
                ws("/api"),
                worktree("/api"),
                session("a"),
                session("b"),
                worktree(OAUTH),
                session("o"),
                ws("/tmp"),
                session("z"),
                ws("/web"),
                session("c"),
            ]
        );
    }

    #[test]
    fn after_a_tab_closes_the_worktree_s_first_session_is_shown() {
        assert_eq!(
            tab_after_closing(&tree(), &at("/api"), &[7]),
            Some(Tab::Session("a".into()))
        );
    }

    #[test]
    fn a_worktree_without_sessions_shows_another_shell_or_nothing() {
        assert_eq!(
            tab_after_closing(&tree(), &at("/nowhere"), &[7]),
            Some(Tab::Shell(7))
        );
        assert_eq!(tab_after_closing(&tree(), &at("/nowhere"), &[]), None);
    }

    #[test]
    fn the_selection_moves_to_the_new_tab_s_row_only_from_the_old_one_s() {
        let t = tree();
        let list = items(&t, &inbox(&t), &HashSet::new());
        let shell = Tab::Shell(1);
        let a = Tab::Session("a".into());
        let row_a = Some(session("a"));
        let switch = |selected: Option<Item>, before: Option<&Tab>| {
            row_after_switch(&list, &t, selected.as_ref(), before, &a, &at("/api"))
        };
        // On the shell's row, its worktree's, it moves to the session's.
        assert_eq!(switch(Some(worktree("/api")), Some(&shell)), row_a);
        // So it does from nothing, or from the old session's inbox row.
        assert_eq!(switch(None, None), row_a);
        let b = Tab::Session("b".into());
        assert_eq!(switch(Some(Item::Waiting("b".into())), Some(&b)), row_a);
        // Moved elsewhere by hand, it stays.
        assert_eq!(switch(Some(ws("/web")), Some(&shell)), None);
        // On a row no longer listed, it moves too.
        let gone = Tab::Session("gone".into());
        assert_eq!(switch(Some(session("gone")), Some(&gone)), row_a);
    }

    #[test]
    fn a_selected_row_that_leaves_gives_way_to_the_same_session_s_or_the_shown_tab_s() {
        let t = tree();
        let list = items(&t, &inbox(&t), &HashSet::new());
        let shown = Tab::Session("a".into());
        let kept = |row: Item| row_kept(&list, &t, Some(&row), Some(&shown), &at("/api"));
        // "a" no longer waits: its inbox row leaves for its own.
        assert_eq!(kept(Item::Waiting("a".into())), Some(session("a")));
        // A row still listed stays.
        assert_eq!(kept(ws("/web")), None);
        // A session gone gives way to the shown tab's row.
        assert_eq!(kept(session("gone")), Some(session("a")));
    }

    #[test]
    fn a_filter_keeps_the_sessions_whose_names_match_and_the_rows_above_them() {
        let t = tree();
        let list = items(&narrowed(&t, "O"), &[], &HashSet::new());
        // The worktree oauth is named with an o, and keeps all it holds.
        assert_eq!(list, [ws("/api"), worktree(OAUTH), session("o")]);
    }

    #[test]
    fn an_empty_filter_keeps_everything_and_one_matching_nothing_keeps_nothing() {
        let t = tree();
        assert_eq!(narrowed(&t, ""), t);
        assert_eq!(items(&narrowed(&t, "nothing"), &[], &HashSet::new()), []);
    }

    #[test]
    fn a_filter_naming_a_workspace_keeps_all_beneath_it() {
        let t = tree();
        let web = items(&narrowed(&t, "web"), &[], &HashSet::new());
        assert_eq!(web, [ws("/web"), session("c")]);
    }

    #[test]
    fn a_prompt_sent_with_nothing_open_goes_where_it_can_only_mean_or_to_scratch() {
        let scratch = Path::new("/scratch");
        // The worktree last opened wins.
        assert_eq!(start_place(&tree(), Some(&at("/w")), scratch), at("/w"));
        // Else the only place sessions run.
        let one = build(
            &[s("a", "/x", State::Idle)],
            &[],
            &HashMap::new(),
            &[],
            None,
        );
        assert_eq!(start_place(&one, None, scratch), at("/x"));
        // A worktree listed without sessions is no place sessions run.
        let found = HashMap::from([(PathBuf::from("/api"), Some(wt("/api", "/api", "main")))]);
        let listed = build(
            &[s("a", "/api", State::Idle)],
            &[],
            &found,
            &[wt("/api", "/api", "main"), wt(OAUTH, "/api", "oauth")],
            None,
        );
        assert_eq!(listed.worktrees().count(), 2);
        assert_eq!(start_place(&listed, None, scratch), at("/api"));
        // With more than one place to mean, or none, a scratch session starts.
        assert_eq!(start_place(&tree(), None, scratch), at("/scratch"));
        assert_eq!(start_place(&Tree::default(), None, scratch), at("/scratch"));
    }

    #[test]
    fn a_closed_tab_gives_way_to_the_one_before_it_or_else_after_it() {
        let a = Tab::Session("a".into());
        let b = Tab::Shell(1);
        let c = Tab::Session("c".into());
        let tabs = [a.clone(), b.clone(), c.clone()];
        assert_eq!(tab_after(&tabs, &b), Some(a.clone()));
        assert_eq!(tab_after(&tabs, &a), Some(b));
        assert_eq!(tab_after(std::slice::from_ref(&c), &c), None);
        assert_eq!(tab_after(&tabs, &Tab::Shell(9)), None);
    }

    #[test]
    fn a_session_is_gone_when_neither_it_nor_a_successor_is_listed() {
        let t = tree();
        let sessions: Vec<Session> = t.sessions().map(|(_, s)| s.clone()).collect();
        assert!(!is_gone(&sessions, "a"));
        assert!(is_gone(&sessions, "nowhere"));
        let moved = Session {
            id: "job".into(),
            session_id: "uuid".into(),
            ..Default::default()
        };
        assert!(!is_gone(&[moved], "uuid"));
    }

    #[test]
    fn a_session_tab_selects_the_session_s_row() {
        let t = tree();
        let list = items(&t, &inbox(&t), &HashSet::new());
        assert_eq!(
            row_of(&list, &t, Some(&Tab::Session("b".into())), &at("/api")),
            Some(session("b"))
        );
    }

    #[test]
    fn a_tab_without_a_session_selects_its_worktree() {
        let t = tree();
        let list = items(&t, &[], &HashSet::new());
        assert_eq!(row_of(&list, &t, None, &at("/api")), Some(worktree("/api")));
    }

    #[test]
    fn a_session_folded_away_selects_the_nearest_row_still_shown() {
        let t = tree();
        let mut collapsed = HashSet::new();
        for row in [worktree("/api"), ws("/api")] {
            collapsed.insert(row.fold_key().unwrap());
            let list = items(&t, &[], &collapsed);
            assert_eq!(
                row_of(&list, &t, Some(&Tab::Session("a".into())), &at("/api")),
                Some(row)
            );
        }
    }

    #[test]
    fn a_session_of_a_flat_workspace_folded_away_selects_the_workspace() {
        let t = tree();
        let collapsed = HashSet::from([ws("/tmp").fold_key().unwrap()]);
        let list = items(&t, &[], &collapsed);
        assert_eq!(
            row_of(&list, &t, Some(&Tab::Session("z".into())), &at("/tmp")),
            Some(ws("/tmp"))
        );
    }

    #[test]
    fn a_collapsed_node_hides_its_children() {
        let t = tree();
        let collapsed = HashSet::from([worktree("/api").fold_key().unwrap()]);
        let got = items(&t, &[], &collapsed);
        assert!(got.contains(&worktree("/api")));
        assert!(!got.contains(&session("a")));
        assert!(got.contains(&session("o")));
    }

    #[test]
    fn stepping_clamps_at_both_ends() {
        let list = vec![ws("/w"), session("x")];
        assert_eq!(step(&list, None, 1), Some(ws("/w")));
        assert_eq!(step(&list, Some(&ws("/w")), 1), Some(session("x")));
        assert_eq!(step(&list, Some(&session("x")), 1), Some(session("x")));
        assert_eq!(step(&list, Some(&ws("/w")), -1), Some(ws("/w")));
        assert_eq!(step(&[], None, 1), None);
    }

    #[test]
    fn wrapping_goes_round_both_ends() {
        assert_eq!(wrap(2, 1, 3), 0);
        assert_eq!(wrap(0, -1, 3), 2);
        assert_eq!(wrap(1, 4, 3), 2);
    }

    #[test]
    fn the_next_waiting_session_wraps_around() {
        let t = tree();
        let ib = inbox(&t);
        assert_eq!(next_waiting(&ib, None, true).as_deref(), Some("b"));
        assert_eq!(next_waiting(&ib, Some("b"), true).as_deref(), Some("z"));
        assert_eq!(next_waiting(&ib, Some("z"), true).as_deref(), Some("b"));
        assert_eq!(next_waiting(&ib, Some("b"), false).as_deref(), Some("z"));
    }

    #[test]
    fn the_next_session_goes_down_the_tree_and_wraps_around() {
        let t = tree();
        let shown = |id: &str| Some(Tab::Session(id.into()));
        let next = |id: &str, forward| next_session(&t, shown(id).as_ref(), None, forward);
        for (from, forward, to) in [
            ("a", true, "b"),
            ("b", true, "o"),
            ("o", true, "z"),
            ("z", true, "c"),
            ("c", true, "a"),
            ("a", false, "c"),
            ("o", false, "b"),
        ] {
            assert_eq!(next(from, forward).as_deref(), Some(to), "{from} {forward}");
        }
    }

    #[test]
    fn a_tab_without_a_session_stands_after_the_sessions_of_its_worktree() {
        let t = tree();
        let shell = Some(Tab::Shell(1));
        let next = |path: &str, forward| next_session(&t, shell.as_ref(), Some(&at(path)), forward);
        assert_eq!(next("/api", true).as_deref(), Some("o"));
        assert_eq!(next("/api", false).as_deref(), Some("b"));
        assert_eq!(next(OAUTH, true).as_deref(), Some("z"));
        assert_eq!(next("/web", true).as_deref(), Some("a"));
        // A session gone stands where its tab is, as a shell's does.
        let gone = Some(Tab::Session("gone".into()));
        let after = next_session(&t, gone.as_ref(), Some(&at("/api")), true);
        assert_eq!(after.as_deref(), Some("o"));
    }

    #[test]
    fn with_nothing_shown_the_next_session_is_the_first_and_the_previous_the_last() {
        let t = tree();
        assert_eq!(next_session(&t, None, None, true).as_deref(), Some("a"));
        assert_eq!(next_session(&t, None, None, false).as_deref(), Some("c"));
        // So it is from a place no session runs in.
        let nowhere = Some(at("/nowhere"));
        let draft = next_session(&t, None, nowhere.as_ref(), true);
        assert_eq!(draft.as_deref(), Some("a"));
        assert_eq!(next_session(&Tree::default(), None, None, true), None);
    }

    #[test]
    fn a_session_opens_its_worktree_with_the_sessions_there_as_tabs() {
        let t = tree();
        let place = place_of(&t, &session("a")).unwrap();
        assert_eq!(place, at("/api"));
        let tabs: Vec<_> = sessions_in(&t, &place)
            .iter()
            .map(|s| s.id.clone())
            .collect();
        assert_eq!(tabs, vec!["a", "b"]);
    }

    #[test]
    fn a_workspace_works_in_its_main_checkout_or_its_own_directory() {
        let t = tree();
        assert_eq!(place_of(&t, &ws("/api")), Some(at("/api")));
        assert_eq!(place_of(&t, &ws("/tmp")), Some(at("/tmp")));
        let root = |item: Item| {
            let place = place_of(&t, &item)?;
            t.workspace_of(&place.worktree).map(|w| w.root.clone())
        };
        assert_eq!(root(session("o")), Some(PathBuf::from("/api")));
        assert_eq!(root(worktree(OAUTH)), Some(PathBuf::from("/api")));
    }

    #[test]
    fn a_workspace_without_a_main_checkout_works_in_its_first_worktree() {
        // A bare repository, a submodule or a separate git directory is
        // named by its git directory, which no session should start in.
        let found = HashMap::from([(
            PathBuf::from("/src/w1"),
            Some(wt("/src/w1", "/src/api.git", "a")),
        )]);
        let t = build(&[s("a", "/src/w1", State::Idle)], &[], &found, &[], None);
        assert_eq!(place_of(&t, &ws("/src/api.git")), Some(at("/src/w1")));
    }

    const DEV: &str = "/api/.claude/worktrees/dev";

    fn sh(id: u128, name: &str, dir: &str) -> Shell {
        Shell {
            runs: Default::default(),
            id,
            name: name.into(),
            worktree: dir.into(),
        }
    }

    /// `tree()` with shells: one beside `api`'s sessions, two in a worktree
    /// where no session runs, one in `tmp`, a workspace without a worktree
    /// row, and one in `notes`, outside git, where nothing else runs.
    fn shelled() -> Tree {
        let found: HashMap<PathBuf, Option<Worktree>> = [
            ("/api", Some(wt("/api", "/api", "main"))),
            (OAUTH, Some(wt(OAUTH, "/api", "oauth"))),
            (DEV, Some(wt(DEV, "/api", "dev"))),
            ("/web", Some(wt("/web", "/web", "main"))),
            ("/tmp", None),
            ("/notes", None),
        ]
        .into_iter()
        .map(|(p, w)| (PathBuf::from(p), w))
        .collect();
        let sessions: Vec<Session> = tree().sessions().map(|(_, s)| s.clone()).collect();
        build(
            &sessions,
            &[
                sh(1, "shell", "/api"),
                sh(2, "shell", DEV),
                sh(3, "shell-2", DEV),
                sh(4, "shell", "/tmp"),
                sh(5, "logs", "/notes"),
            ],
            &found,
            &[],
            None,
        )
    }

    fn shell(id: u128) -> Item {
        Item::Shell(id)
    }

    #[test]
    fn each_shell_has_a_row_after_the_sessions_of_its_worktree() {
        let got = items(&shelled(), &[], &HashSet::new());
        assert_eq!(
            got,
            vec![
                ws("/api"),
                worktree("/api"),
                session("a"),
                session("b"),
                shell(1),
                worktree(DEV),
                shell(2),
                shell(3),
                worktree(OAUTH),
                session("o"),
                ws("/notes"),
                shell(5),
                ws("/tmp"),
                session("z"),
                shell(4),
                ws("/web"),
                session("c"),
            ]
        );
    }

    #[test]
    fn folding_a_worktree_or_a_workspace_hides_its_shells() {
        let t = shelled();
        let folded = |row: Item| {
            let collapsed = HashSet::from([row.fold_key().unwrap()]);
            items(&t, &[], &collapsed)
        };
        let list = folded(worktree(DEV));
        assert!(list.contains(&worktree(DEV)) && !list.contains(&shell(2)));
        assert!(!folded(ws("/tmp")).contains(&shell(4)));
    }

    #[test]
    fn a_filter_keeps_the_shells_whose_names_match_as_it_keeps_sessions() {
        let t = shelled();
        let list = items(&narrowed(&t, "LOG"), &[], &HashSet::new());
        assert_eq!(list, [ws("/notes"), shell(5)]);
        // A worktree named by the filter keeps every shell in it.
        let dev = items(&narrowed(&t, "dev"), &[], &HashSet::new());
        assert_eq!(dev, [ws("/api"), worktree(DEV), shell(2), shell(3)]);
        let named = items(&narrowed(&t, "shell-2"), &[], &HashSet::new());
        assert_eq!(named, [ws("/api"), worktree(DEV), shell(3)]);
    }

    #[test]
    fn a_shell_tab_selects_the_shell_s_row_or_the_nearest_one_shown() {
        let t = shelled();
        let tab = Tab::Shell(2);
        let list = items(&t, &[], &HashSet::new());
        assert_eq!(row_of(&list, &t, Some(&tab), &at(DEV)), Some(shell(2)));
        let collapsed = HashSet::from([worktree(DEV).fold_key().unwrap()]);
        let folded = items(&t, &[], &collapsed);
        assert_eq!(
            row_of(&folded, &t, Some(&tab), &at(DEV)),
            Some(worktree(DEV))
        );
    }

    #[test]
    fn the_selection_moves_between_a_session_s_row_and_a_shell_s_with_the_tab() {
        let t = shelled();
        let list = items(&t, &[], &HashSet::new());
        let a = Tab::Session("a".into());
        let one = Tab::Shell(1);
        let switch = |selected: Item, before: &Tab, now: &Tab| {
            row_after_switch(&list, &t, Some(&selected), Some(before), now, &at("/api"))
        };
        assert_eq!(switch(session("a"), &a, &one), Some(shell(1)));
        assert_eq!(switch(shell(1), &one, &a), Some(session("a")));
        // A shell gone, its row with it, gives way to the tab shown next.
        assert_eq!(switch(shell(9), &Tab::Shell(9), &a), Some(session("a")));
        assert_eq!(switch(ws("/web"), &one, &a), None);
    }

    #[test]
    fn a_selected_shell_that_ends_gives_way_to_the_shown_tab_s_row() {
        let t = shelled();
        let list = items(&t, &[], &HashSet::new());
        let shown = Tab::Session("a".into());
        let kept = row_kept(&list, &t, Some(&shell(9)), Some(&shown), &at("/api"));
        assert_eq!(kept, Some(session("a")));
    }

    #[test]
    fn a_shell_opens_the_directory_it_runs_in() {
        let t = shelled();
        assert_eq!(place_of(&t, &shell(2)), Some(at(DEV)));
        assert_eq!(place_of(&t, &shell(5)), Some(at("/notes")));
        assert_eq!(place_of(&t, &shell(9)), None);
    }

    #[test]
    fn a_closed_row_gives_way_to_the_one_above_it() {
        let list = items(&shelled(), &[], &HashSet::new());
        assert_eq!(row_above(&list, &shell(3)), Some(shell(2)));
        assert_eq!(row_above(&list, &shell(2)), Some(worktree(DEV)));
        assert_eq!(row_above(&list, &ws("/api")), None);
        assert_eq!(row_above(&list, &shell(9)), None);
    }

    #[test]
    fn shells_are_passed_over_going_from_session_to_session() {
        let t = shelled();
        let shown = Some(Tab::Session("b".into()));
        let next = next_session(&t, shown.as_ref(), None, true);
        assert_eq!(next.as_deref(), Some("o"));
        // Nor is a worktree with only shells a place sessions run.
        let notes = build(&[], &[sh(1, "shell", "/notes")], &HashMap::new(), &[], None);
        assert_eq!(
            start_place(&notes, None, Path::new("/scratch")),
            at("/scratch")
        );
    }

    fn listed(id: &str, session_id: &str) -> Session {
        Session {
            id: id.into(),
            session_id: session_id.into(),
            ..Default::default()
        }
    }

    #[test]
    fn a_session_taken_over_is_found_under_its_new_id() {
        let sessions = [listed("a08bbcf2", "a08bbcf2-2cbf")];
        assert_eq!(successor(&sessions, "a08bbcf2-2cbf"), Some("a08bbcf2"));
    }

    #[test]
    fn a_session_still_listed_or_gone_has_no_successor() {
        let sessions = [listed("a08bbcf2-2cbf", "a08bbcf2-2cbf")];
        assert_eq!(successor(&sessions, "a08bbcf2-2cbf"), None);
        assert_eq!(successor(&sessions, "ffffffff-0000"), None);
    }

    #[test]
    fn tabs_kept_for_sessions_follow_them_and_go_with_them() {
        let sessions = [listed("a08bbcf2", "a08bbcf2-2cbf"), listed("b", "b")];
        let kept: HashSet<Tab> = [
            Tab::Session("a08bbcf2-2cbf".into()),
            Tab::Session("b".into()),
            Tab::Session("gone".into()),
            Tab::Shell(1),
        ]
        .into();
        let expected: HashSet<Tab> = [
            Tab::Session("a08bbcf2".into()),
            Tab::Session("b".into()),
            Tab::Shell(1),
        ]
        .into();
        assert_eq!(followed(&sessions, &kept), expected);
    }

    #[test]
    fn the_row_of_a_session_taken_over_follows_it() {
        let sessions = [listed("a08bbcf2", "a08bbcf2-2cbf")];
        let moved = Some(session("a08bbcf2"));
        assert_eq!(successor_row(&sessions, &session("a08bbcf2-2cbf")), moved);
        assert_eq!(
            successor_row(&sessions, &Item::Waiting("a08bbcf2-2cbf".into())),
            moved
        );
    }

    #[test]
    fn an_inbox_row_stays_in_the_inbox_while_its_moved_session_waits() {
        let sessions = [Session {
            state: State::Waiting(Wait::Input),
            ..listed("a08bbcf2", "a08bbcf2-2cbf")
        }];
        assert_eq!(
            successor_row(&sessions, &Item::Waiting("a08bbcf2-2cbf".into())),
            Some(Item::Waiting("a08bbcf2".into()))
        );
        assert_eq!(
            successor_row(&sessions, &session("a08bbcf2-2cbf")),
            Some(session("a08bbcf2"))
        );
    }

    #[test]
    fn a_row_not_for_a_moved_session_has_no_successor() {
        let sessions = [listed("a08bbcf2", "a08bbcf2-2cbf")];
        assert_eq!(successor_row(&sessions, &session("a08bbcf2")), None);
        assert_eq!(successor_row(&sessions, &ws("/api")), None);
    }
}
