use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::{Path, PathBuf};

use adroit_model::files::Entry;
use adroit_model::github::{Listing, Repository};
use adroit_model::testing::{self, TreeBuilder};
use adroit_model::{EnvironmentId, Session, TerminalProgram, TerminalSession, Tree, Wait, inbox};

use super::*;
use crate::content::Tab;
use crate::testing::here;

#[test]
fn the_same_path_in_two_environments_is_two_rows_folded_apart() {
    let here = Item::Worktree(EnvironmentId::new("here"), "/w".into());
    let away = Item::Worktree(EnvironmentId::new("away"), "/w".into());
    assert_ne!(here, away);
    let mut folds = Folds::default();
    folds.set(&here, true);
    assert!(folds.is_open(&here));
    assert!(!folds.is_open(&away));
}

#[test]
fn the_same_session_in_two_environments_is_two_rows() {
    let here = EnvironmentId::new("here");
    let away = EnvironmentId::new("away");
    assert_ne!(
        Item::Session(here.clone(), "s".into()),
        Item::Session(away.clone(), "s".into())
    );
    assert_ne!(Item::Terminal(here, 1), Item::Terminal(away, 1));
}

const OAUTH: &str = "/api/.claude/worktrees/oauth";

/// `api` is a repository with sessions in its main checkout and in a
/// worktree; `web` one with sessions only in its main checkout; `tmp` is
/// outside git.
fn tree() -> Tree {
    repositories()
        .found("/tmp", None)
        .session(testing::session("a").cwd("/api").working())
        .session(testing::session("b").cwd("/api").waiting(Wait::Permission))
        .session(testing::session("o").cwd(OAUTH))
        .session(testing::session("c").cwd("/web"))
        .session(testing::session("z").cwd("/tmp").waiting(Wait::Input))
        .build()
}

/// The main checkouts of `api` and `web`, and `api`'s `oauth` worktree.
fn repositories() -> TreeBuilder {
    testing::tree()
        .checkout("/api", "/api", "main")
        .checkout(OAUTH, "/api", "oauth")
        .checkout("/web", "/web", "main")
}

fn ws(root: &str) -> Item {
    Item::Workspace(here(), root.into())
}

fn worktree(path: &str) -> Item {
    Item::Worktree(here(), path.into())
}

fn session(id: &str) -> Item {
    Item::Session(here(), id.into())
}

fn sessions_of(path: &str) -> Item {
    Item::Heading(here(), path.into(), Heading::Sessions)
}

fn shells_of(path: &str) -> Item {
    Item::Heading(here(), path.into(), Heading::Terminals)
}

fn files_of(path: &str) -> Item {
    Item::Heading(here(), path.into(), Heading::Files)
}

fn across(root: &str, heading: Heading) -> Item {
    Item::Across(here(), root.into(), heading)
}

fn gathered_session(id: &str) -> Item {
    Item::Gathered(Box::new(session(id)))
}

/// A repository's headings over every worktree, with the rows under each.
fn gathered(root: &str, sessions: &[&str], shells: &[u128]) -> Vec<Item> {
    [
        vec![across(root, Heading::Sessions)],
        sessions.iter().map(|id| gathered_session(id)).collect(),
        vec![across(root, Heading::Terminals)],
        shells
            .iter()
            .map(|&id| Item::Gathered(Box::new(Item::Terminal(here(), id))))
            .collect(),
    ]
    .concat()
}

#[test]
fn a_repository_s_worktrees_start_folded_and_unfold_to_their_own_headings() {
    let t = tree();
    let mut folds = Folds::default();
    folds.toggle(&worktree(OAUTH));
    let got = rows(&t, &folds);
    let at = got.iter().position(|i| *i == worktree(OAUTH)).unwrap();
    assert_eq!(got[at - 1], worktree("/api"));
    assert_eq!(got[at + 1..at + 5], group(OAUTH, &[session("o")], &[])[..]);
    assert_eq!(depth(&t, &across("/api", Heading::Sessions)), 1);
    assert_eq!(depth(&t, &gathered_session("o")), 2);
    assert_eq!(depth(&t, &sessions_of(OAUTH)), 2);
    assert_eq!(depth(&t, &session("o")), 3);
}

#[test]
fn the_row_of_a_shown_session_is_its_gathered_row_or_else_its_worktree_s() {
    let t = tree();
    let tab = Tab::Session(here(), "o".into());
    let row = |folds: &Folds| {
        let list = rows(&t, folds);
        row_of(&list, &t, &here(), Some(&tab), &at(OAUTH))
    };
    assert_eq!(row(&Folds::default()), Some(gathered_session("o")));
    let mut folds = closed(&[across("/api", Heading::Sessions)]);
    assert_eq!(row(&folds), Some(across("/api", Heading::Sessions)));
    folds.toggle(&worktree(OAUTH));
    assert_eq!(row(&folds), Some(session("o")));
}

#[test]
fn a_workspace_where_nothing_runs_starts_folded_and_keeps_how_it_was_folded() {
    let t = repositories()
        .checkout("/idle", "/idle", "main")
        .kept("/idle")
        .session(testing::session("c").cwd("/web"))
        .build();
    let mut folds = Folds::default();
    folds.rest(&here(), &t);
    assert!(folds.is_open(&ws("/web")));
    assert!(!folds.is_open(&ws("/idle")));
    let got = rows(&t, &folds);
    assert!(got.contains(&ws("/idle")));
    assert!(!got.contains(&sessions_of("/idle")));
    assert!(got.contains(&sessions_of("/web")));

    folds.toggle(&ws("/idle"));
    folds.toggle(&ws("/web"));
    let busy = repositories()
        .checkout("/idle", "/idle", "main")
        .kept("/idle")
        .session(testing::session("i").cwd("/idle"))
        .build();
    folds.rest(&here(), &busy);
    assert!(folds.is_open(&ws("/idle")));
    let t = repositories()
        .checkout("/idle", "/idle", "main")
        .kept("/idle")
        .build();
    folds.rest(&here(), &t);
    assert!(folds.is_open(&ws("/idle")));
    assert!(!folds.is_open(&ws("/web")));
}

#[test]
fn a_workspace_comes_open_once_something_runs_in_it() {
    let idle = repositories()
        .checkout("/idle", "/idle", "main")
        .kept("/idle")
        .build();
    let busy = repositories()
        .checkout("/idle", "/idle", "main")
        .kept("/idle")
        .terminal(testing::shell(1, "/idle"))
        .build();
    let mut folds = Folds::default();
    folds.rest(&here(), &idle);
    assert!(!folds.is_open(&ws("/idle")));
    folds.rest(&here(), &busy);
    assert!(folds.is_open(&ws("/idle")));
}

#[test]
fn a_gathered_row_opens_and_copies_what_its_own_row_does() {
    let t = tree();
    let row = gathered_session("o");
    assert_eq!(
        opens(&row, &t, &[], true),
        opens(&session("o"), &t, &[], true)
    );
    assert_eq!(copied(&row, &t, &[]), copied(&session("o"), &t, &[]));
    assert_eq!(stands_for(&row, &[]), stands_for(&session("o"), &[]));
    assert_eq!(place_of(&t, &row), Some(at(OAUTH)));
    assert!(!holds(&row));
}

/// A worktree's headings with the rows under each, its files folded.
fn group(path: &str, sessions: &[Item], shells: &[Item]) -> Vec<Item> {
    [
        &[files_of(path), sessions_of(path)][..],
        sessions,
        &[shells_of(path)],
        shells,
    ]
    .concat()
}

/// The rows of `tree` with no file anywhere.
fn rows(tree: &Tree, folds: &Folds) -> Vec<Item> {
    items(&here(), tree, folds, &|_| Vec::new(), &BTreeMap::new())
}

/// The rows of `tree` followed, as the tree view lists them, by those of
/// the sessions waiting on the user.
fn rows_and_waiting(tree: &Tree, folds: &Folds) -> Vec<Item> {
    let waiting = inbox(tree)
        .into_iter()
        .map(|i| Item::Waiting(here(), i.session.id));
    rows(tree, folds).into_iter().chain(waiting).collect()
}

fn file(worktree: &str, path: &str, dir: bool) -> Item {
    Item::File {
        environment: here(),
        worktree: worktree.into(),
        path: path.into(),
        dir,
    }
}

/// A worktree at `/web` holding `src/` with `main.rs` in it, and
/// `README.md`.
fn read_web(dir: &Path) -> Vec<Entry> {
    let entry = |path: &str, dir: bool| Entry {
        path: path.into(),
        dir,
    };
    match dir.to_str() {
        Some("/web") => vec![entry("/web/src", true), entry("/web/README.md", false)],
        Some("/web/src") => vec![entry("/web/src/main.rs", false)],
        _ => Vec::new(),
    }
}

fn on_github(root: &str, listing: Listing) -> Item {
    Item::GitHub(here(), root.into(), listing)
}

/// `/api` and `/web` on GitHub, `api`'s pull requests read and its
/// issues failed, `web` not read yet.
fn github() -> BTreeMap<PathBuf, Repository> {
    let item = |number: u64| adroit_model::github::Item {
        number,
        title: format!("#{number}"),
        url: format!("https://github.com/o/api/pull/{number}"),
        author: "me".into(),
        draft: false,
        ..Default::default()
    };
    let mut api = Repository::new("o/api");
    api.pulls = Some(Ok(vec![item(2), item(1)]));
    api.issues = Some(Err("gh is not logged in".into()));
    BTreeMap::from([
        ("/api".into(), api),
        ("/web".into(), Repository::new("o/web")),
    ])
}

#[test]
fn a_repository_on_github_lists_its_listings_folded_after_what_runs_before_its_worktrees() {
    let t = tree();
    let got = items(&here(), &t, &Folds::default(), &|_| Vec::new(), &github());
    let at = got
        .iter()
        .position(|i| *i == on_github("/api", Listing::Pulls))
        .unwrap();
    assert_eq!(got[at - 1], across("/api", Heading::Terminals));
    assert_eq!(
        got[at + 1..at + 3],
        [on_github("/api", Listing::Issues), worktree("/api")]
    );
    assert_eq!(depth(&t, &on_github("/api", Listing::Pulls)), 1);
}

#[test]
fn a_flat_repository_lists_its_listings_after_its_checkout_s_headings() {
    let t = tree();
    let got = items(&here(), &t, &Folds::default(), &|_| Vec::new(), &github());
    let at = got
        .iter()
        .position(|i| *i == on_github("/web", Listing::Pulls))
        .unwrap();
    assert_eq!(got[at - 1], shells_of("/web"));
    assert_eq!(depth(&t, &on_github("/web", Listing::Pulls)), 1);
}

#[test]
fn a_repository_not_on_github_lists_no_listing() {
    let t = tree();
    let got = rows(&t, &Folds::default());
    assert!(!got.iter().any(|i| matches!(i, Item::GitHub(..))));
}

#[test]
fn an_unfolded_listing_holds_what_was_read_or_one_row_saying_why_it_was_not() {
    let t = tree();
    let mut folds = Folds::default();
    for item in [
        on_github("/api", Listing::Pulls),
        on_github("/api", Listing::Issues),
        on_github("/web", Listing::Pulls),
    ] {
        folds.set(&item, true);
    }
    let got = items(&here(), &t, &folds, &|_| Vec::new(), &github());
    let api = got
        .iter()
        .position(|i| *i == on_github("/api", Listing::Pulls))
        .unwrap();
    let pull = |n| Item::GitHubItem(here(), "/api".into(), Listing::Pulls, n);
    assert_eq!(
        got[api + 1..api + 5],
        [
            pull(2),
            pull(1),
            on_github("/api", Listing::Issues),
            Item::GitHubFailed(here(), "/api".into(), Listing::Issues),
        ]
    );
    assert_eq!(depth(&t, &pull(2)), 2);
    let web = got
        .iter()
        .position(|i| *i == on_github("/web", Listing::Pulls))
        .unwrap();
    assert_eq!(
        got[web + 1],
        on_github("/web", Listing::Issues),
        "not read yet"
    );
    assert_eq!(place_of(&t, &pull(2)), Some(at("/api")));
    assert!(!holds(&pull(2)));
}

#[test]
fn a_worktree_s_files_are_listed_once_its_files_heading_is_unfolded() {
    let t = tree();
    let folded = items(&here(), &t, &Folds::default(), &read_web, &BTreeMap::new());
    assert!(!folded.iter().any(|i| matches!(i, Item::File { .. })));
    let mut folds = Folds::default();
    folds.toggle(&files_of("/web"));
    let open = items(&here(), &t, &folds, &read_web, &BTreeMap::new());
    let at = open.iter().position(|i| *i == files_of("/web")).unwrap();
    assert_eq!(
        open[at + 1..at + 3],
        [
            file("/web", "/web/src", true),
            file("/web", "/web/README.md", false)
        ]
    );
    assert_eq!(open[at + 3], sessions_of("/web"));
}

#[test]
fn an_unfolded_directory_lists_its_entries_beneath_it_one_level_deeper() {
    let t = tree();
    let mut folds = Folds::default();
    folds.toggle(&files_of("/web"));
    folds.toggle(&file("/web", "/web/src", true));
    let open = items(&here(), &t, &folds, &read_web, &BTreeMap::new());
    let at = open
        .iter()
        .position(|i| *i == file("/web", "/web/src", true))
        .unwrap();
    assert_eq!(open[at + 1], file("/web", "/web/src/main.rs", false));
    assert_eq!(depth(&t, &files_of("/web")), 1);
    assert_eq!(depth(&t, &file("/web", "/web/src", true)), 2);
    assert_eq!(depth(&t, &file("/web", "/web/src/main.rs", false)), 3);
}

#[test]
fn a_row_s_parent_is_the_nearest_row_above_it_one_level_up() {
    let t = tree();
    let mut folds = Folds::default();
    folds.toggle(&files_of("/web"));
    folds.toggle(&file("/web", "/web/src", true));
    let list = items(&here(), &t, &folds, &read_web, &BTreeMap::new());
    let main = file("/web", "/web/src/main.rs", false);
    assert_eq!(
        parent(&list, |i| depth(&t, i), &main),
        Some(file("/web", "/web/src", true))
    );
    let readme = file("/web", "/web/README.md", false);
    assert_eq!(
        parent(&list, |i| depth(&t, i), &readme),
        Some(files_of("/web"))
    );
    assert_eq!(
        parent(&list, |i| depth(&t, i), &files_of("/web")),
        Some(ws("/web"))
    );
    assert_eq!(parent(&list, |i| depth(&t, i), &ws("/web")), None);
}

fn opened(rows: &[Item]) -> Folds {
    let mut folds = Folds::default();
    for row in rows {
        folds.set(row, true);
    }
    folds
}

fn closed(rows: &[Item]) -> Folds {
    let mut folds = Folds::default();
    for row in rows {
        folds.set(row, false);
    }
    folds
}

fn at(path: &str) -> Place {
    Place::at(&here(), path)
}

#[test]
fn each_workspace_lists_its_worktrees_headings_and_rows() {
    let t = tree();
    let got = rows(&t, &Folds::default());
    let expected = [
        vec![ws("/api")],
        gathered("/api", &["a", "b", "o"], &[]),
        vec![worktree("/api"), worktree(OAUTH)],
        vec![ws("/tmp")],
        group("/tmp", &[session("z")], &[]),
        vec![ws("/web")],
        group("/web", &[session("c")], &[]),
    ]
    .concat();
    assert_eq!(got, expected);
}

/// `a` runs in the main checkout of `api` and opened `o`, which runs in
/// the `oauth` worktree, and `w`, which runs in `web`.
fn opening_tree(opener_listed: bool) -> Tree {
    let opener = adroit_model::SessionId::new();
    let opened = |id: &str, cwd: &str| {
        testing::session(id)
            .cwd(cwd)
            .working()
            .delegation(adroit_model::SessionId::new())
            .parent(opener)
    };
    let t = repositories()
        .session(opened("o", OAUTH))
        .session(opened("w", "/web"));
    let a = testing::session("a")
        .cwd("/api")
        .working()
        .delegation(opener);
    if opener_listed { t.session(a) } else { t }.build()
}

#[test]
fn an_opened_session_is_listed_where_it_runs_whether_the_one_that_opened_it_is_listed_or_not() {
    let web = [vec![ws("/web")], group("/web", &[session("w")], &[])].concat();
    assert_eq!(
        rows(&opening_tree(true), &Folds::default()),
        [
            vec![ws("/api")],
            gathered("/api", &["a", "o"], &[]),
            vec![worktree("/api"), worktree(OAUTH)],
            web.clone(),
        ]
        .concat()
    );
    assert_eq!(
        rows(&opening_tree(false), &Folds::default()),
        [
            vec![ws("/api")],
            gathered("/api", &["o"], &[]),
            vec![worktree(OAUTH)],
            web,
        ]
        .concat()
    );
}

#[test]
fn after_a_tab_closes_the_worktree_s_first_session_is_shown() {
    assert_eq!(
        tab_after_closing(&here(), &tree(), &at("/api"), &[7]),
        Some(Tab::Session(here(), "a".into()))
    );
}

#[test]
fn a_worktree_without_sessions_shows_another_shell_or_nothing() {
    assert_eq!(
        tab_after_closing(&here(), &tree(), &at("/nowhere"), &[7]),
        Some(Tab::Terminal(here(), 7))
    );
    assert_eq!(
        tab_after_closing(&here(), &tree(), &at("/nowhere"), &[]),
        None
    );
}

#[test]
fn the_selection_moves_to_the_new_tab_s_row_only_from_the_old_one_s() {
    let t = tree();
    let list = rows_and_waiting(&t, &Folds::default());
    let shell = Tab::Terminal(here(), 1);
    let a = Tab::Session(here(), "a".into());
    let row_a = Some(gathered_session("a"));
    let switch = |selected: Option<Item>, before: Option<&Tab>| {
        row_after_switch(&list, &t, selected.as_ref(), before, &a, &at("/api"))
    };
    // On the shell's row, its worktree's, it moves to the session's.
    assert_eq!(switch(Some(worktree("/api")), Some(&shell)), row_a);
    // So it does from nothing, or from the old session's inbox row.
    assert_eq!(switch(None, None), row_a);
    let b = Tab::Session(here(), "b".into());
    assert_eq!(
        switch(Some(Item::Waiting(here(), "b".into())), Some(&b)),
        row_a
    );
    // Moved elsewhere by hand, it stays.
    assert_eq!(switch(Some(ws("/web")), Some(&shell)), None);
    // On a row no longer listed, it moves too.
    let gone = Tab::Session(here(), "gone".into());
    assert_eq!(switch(Some(session("gone")), Some(&gone)), row_a);
}

#[test]
fn a_selected_row_that_leaves_gives_way_to_the_same_session_s_or_the_shown_tab_s() {
    let t = tree();
    let list = rows_and_waiting(&t, &Folds::default());
    let shown = Tab::Session(here(), "a".into());
    let kept = |row: Item| row_kept(&list, &t, Some(&row), Some(&shown), &at("/api"));
    // "a" no longer waits: its inbox row leaves for its own.
    assert_eq!(
        kept(Item::Waiting(here(), "a".into())),
        Some(gathered_session("a"))
    );
    // A row still listed stays.
    assert_eq!(kept(ws("/web")), None);
    // A session gone gives way to the shown tab's row.
    assert_eq!(kept(session("gone")), Some(gathered_session("a")));
}

#[test]
fn a_filter_keeps_the_sessions_whose_names_match_and_the_rows_above_them() {
    let t = tree();
    let list = rows(&narrowed(&t, "o"), &Folds::default());
    // The worktree oauth is named with an o, and keeps all it holds.
    assert_eq!(
        list,
        [
            vec![ws("/api")],
            gathered("/api", &["o"], &[]),
            vec![worktree(OAUTH)],
        ]
        .concat()
    );
}

#[test]
fn an_empty_filter_keeps_everything_and_one_matching_nothing_keeps_nothing() {
    let t = tree();
    assert_eq!(narrowed(&t, ""), t);
    assert_eq!(rows(&narrowed(&t, "nothing"), &Folds::default()), []);
}

#[test]
fn a_filter_naming_a_workspace_keeps_all_beneath_it() {
    let t = tree();
    let web = rows(&narrowed(&t, "web"), &Folds::default());
    assert_eq!(
        web,
        [vec![ws("/web")], group("/web", &[session("c")], &[])].concat()
    );
}

#[test]
fn a_session_is_gone_once_it_is_not_listed() {
    let t = tree();
    let sessions: Vec<Session> = t.sessions().map(|(_, s)| s.clone()).collect();
    assert!(!is_gone(&sessions, "a"));
    assert!(is_gone(&sessions, "nowhere"));
}

#[test]
fn a_session_tab_selects_the_session_s_row() {
    let t = tree();
    let list = rows_and_waiting(&t, &Folds::default());
    assert_eq!(
        row_of(
            &list,
            &t,
            &here(),
            Some(&Tab::Session(here(), "b".into())),
            &at("/api")
        ),
        Some(gathered_session("b"))
    );
}

#[test]
fn a_viewer_selects_its_file_s_row_or_while_folded_away_the_files_heading() {
    let t = tree();
    let viewer = Tab::Viewer(here(), "/web/README.md".into());
    let folded = items(&here(), &t, &Folds::default(), &read_web, &BTreeMap::new());
    assert_eq!(
        row_of(&folded, &t, &here(), Some(&viewer), &at("/web")),
        Some(files_of("/web"))
    );
    let mut folds = Folds::default();
    folds.toggle(&files_of("/web"));
    let open = items(&here(), &t, &folds, &read_web, &BTreeMap::new());
    assert_eq!(
        row_of(&open, &t, &here(), Some(&viewer), &at("/web")),
        Some(file("/web", "/web/README.md", false))
    );
}

#[test]
fn a_tab_without_a_session_selects_its_worktree() {
    let t = tree();
    let list = rows(&t, &Folds::default());
    assert_eq!(
        row_of(&list, &t, &here(), None, &at("/api")),
        Some(worktree("/api"))
    );
}

#[test]
fn a_session_folded_away_selects_the_nearest_row_still_shown() {
    let t = tree();
    for row in [across("/api", Heading::Sessions), ws("/api")] {
        let list = rows(&t, &closed(std::slice::from_ref(&row)));
        assert_eq!(
            row_of(
                &list,
                &t,
                &here(),
                Some(&Tab::Session(here(), "a".into())),
                &at("/api")
            ),
            Some(row)
        );
    }
}

#[test]
fn a_session_of_a_flat_workspace_folded_away_selects_the_workspace() {
    let t = tree();
    let collapsed = closed(&[ws("/tmp")]);
    let list = rows(&t, &collapsed);
    assert_eq!(
        row_of(
            &list,
            &t,
            &here(),
            Some(&Tab::Session(here(), "z".into())),
            &at("/tmp")
        ),
        Some(ws("/tmp"))
    );
}

#[test]
fn a_collapsed_node_hides_its_children() {
    let t = tree();
    let got = rows(&t, &opened(&[worktree(OAUTH)]));
    assert!(got.contains(&worktree("/api")));
    assert!(!got.contains(&session("a")));
    assert!(got.contains(&session("o")));
    let got = rows(&t, &closed(&[across("/api", Heading::Sessions)]));
    assert!(!got.contains(&gathered_session("a")));
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
fn the_next_waiting_session_wraps_around() {
    let t = tree();
    let ib: Vec<_> = inbox(&t).into_iter().map(|i| (here(), i)).collect();
    let next = |current: Option<&str>, forward| {
        next_waiting(&ib, current.map(|id| (&ib[0].0, id)), forward).map(|(_, id)| id)
    };
    assert_eq!(next(None, true).as_deref(), Some("b"));
    assert_eq!(next(Some("b"), true).as_deref(), Some("z"));
    assert_eq!(next(Some("z"), true).as_deref(), Some("b"));
    assert_eq!(next(Some("b"), false).as_deref(), Some("z"));
}

#[test]
fn the_same_session_id_waiting_in_two_environments_is_two_stops() {
    let t = tree();
    let there = EnvironmentId::new("there");
    let ib: Vec<_> = [here(), there.clone()]
        .into_iter()
        .flat_map(|e| inbox(&t).into_iter().map(move |i| (e.clone(), i)))
        .collect();
    assert_eq!(
        next_waiting(&ib, Some((&here(), "z")), true),
        Some((there.clone(), "b".into()))
    );
    assert_eq!(
        next_waiting(&ib, Some((&there, "b")), false),
        Some((here(), "z".into()))
    );
}

#[test]
fn the_next_session_goes_down_the_tree_and_wraps_around() {
    let t = tree();
    let shown = |id: &str| Some(Tab::Session(here(), id.into()));
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
    let shell = Some(Tab::Terminal(here(), 1));
    let next = |path: &str, forward| next_session(&t, shell.as_ref(), Some(&at(path)), forward);
    assert_eq!(next("/api", true).as_deref(), Some("o"));
    assert_eq!(next("/api", false).as_deref(), Some("b"));
    assert_eq!(next(OAUTH, true).as_deref(), Some("z"));
    assert_eq!(next("/web", true).as_deref(), Some("a"));
    // A session gone stands where its tab is, as a shell's does.
    let gone = Some(Tab::Session(here(), "gone".into()));
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
fn the_next_workspace_opens_its_first_session_and_wraps_around() {
    let t = tree();
    let shown = |id: &str| Some(Tab::Session(here(), id.into()));
    let none = HashMap::new();
    let next = |id: &str, forward| next_workspace(&t, shown(id).as_ref(), None, &none, forward);
    for (from, forward, to) in [
        ("a", true, "z"),
        ("o", true, "z"),
        ("z", true, "c"),
        ("c", true, "a"),
        ("a", false, "c"),
        ("o", false, "c"),
        ("c", false, "z"),
    ] {
        assert_eq!(next(from, forward).as_deref(), Some(to), "{from} {forward}");
    }
}

#[test]
fn the_next_workspace_opens_the_session_last_shown_there() {
    let t = tree();
    let shown = Some(Tab::Session(here(), "c".into()));
    let last: HashMap<PathBuf, String> = [(PathBuf::from("/api"), "o".to_string())].into();
    let next = next_workspace(&t, shown.as_ref(), None, &last, true);
    assert_eq!(next.as_deref(), Some("o"));
    // A session last shown that has gone gives way to the first.
    let last: HashMap<PathBuf, String> = [(PathBuf::from("/api"), "gone".to_string())].into();
    let next = next_workspace(&t, shown.as_ref(), None, &last, true);
    assert_eq!(next.as_deref(), Some("a"));
}

#[test]
fn a_tab_without_a_session_goes_from_the_workspace_of_its_worktree() {
    let t = tree();
    let shell = Some(Tab::Terminal(here(), 1));
    let none = HashMap::new();
    let next =
        |path: &str, forward| next_workspace(&t, shell.as_ref(), Some(&at(path)), &none, forward);
    assert_eq!(next(OAUTH, true).as_deref(), Some("z"));
    assert_eq!(next("/web", true).as_deref(), Some("a"));
    assert_eq!(next("/web", false).as_deref(), Some("z"));
    // With nothing shown, the first workspace is next and the last before.
    assert_eq!(
        next_workspace(&t, None, None, &none, true).as_deref(),
        Some("a")
    );
    assert_eq!(
        next_workspace(&t, None, None, &none, false).as_deref(),
        Some("c")
    );
    let nowhere = Some(at("/nowhere"));
    let draft = next_workspace(&t, None, nowhere.as_ref(), &none, true);
    assert_eq!(draft.as_deref(), Some("a"));
    assert_eq!(
        next_workspace(&Tree::default(), None, None, &none, true),
        None
    );
}

#[test]
fn a_workspace_with_no_session_is_passed_over() {
    // `notes` holds only a shell.
    let t = shelled();
    let none = HashMap::new();
    let shell = Some(Tab::Terminal(here(), 5));
    let next = |forward| next_workspace(&t, shell.as_ref(), Some(&at("/notes")), &none, forward);
    assert_eq!(next(true).as_deref(), Some("z"));
    assert_eq!(next(false).as_deref(), Some("a"));
    let shown = Some(Tab::Session(here(), "a".into()));
    let after = next_workspace(&t, shown.as_ref(), None, &none, true);
    assert_eq!(after.as_deref(), Some("z"));
}

#[test]
fn with_one_workspace_the_next_is_the_session_last_shown_in_it() {
    let t = tree();
    let only = Tree {
        workspaces: vec![t.workspaces[0].clone()],
    };
    let shown = Some(Tab::Session(here(), "b".into()));
    let last: HashMap<PathBuf, String> = [(PathBuf::from("/api"), "b".to_string())].into();
    let next = next_workspace(&only, shown.as_ref(), None, &last, true);
    assert_eq!(next.as_deref(), Some("b"));
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
    let t = testing::tree()
        .checkout("/src/w1", "/src/api.git", "a")
        .session(testing::session("a").cwd("/src/w1"))
        .build();
    assert_eq!(place_of(&t, &ws("/src/api.git")), Some(at("/src/w1")));
}

const DEV: &str = "/api/.claude/worktrees/dev";

fn sh(id: u128, name: &str, dir: &str) -> TerminalSession {
    TerminalSession {
        name: name.into(),
        ..testing::shell(id, dir)
    }
}

/// `tree()` with terminals: one beside `api`'s sessions, two in a worktree
/// where no session runs, one in `tmp`, a workspace without a worktree
/// row, and one in `notes`, outside git, where nothing else runs.
fn shelled() -> Tree {
    repositories()
        .checkout(DEV, "/api", "dev")
        .found("/tmp", None)
        .found("/notes", None)
        .sessions(tree().sessions().map(|(_, s)| s.clone()))
        .terminal(sh(1, "shell", "/api"))
        .terminal(sh(2, "shell", DEV))
        .terminal(sh(3, "shell-2", DEV))
        .terminal(sh(4, "shell", "/tmp"))
        .terminal(sh(5, "logs", "/notes"))
        .build()
}

fn shell(id: u128) -> Item {
    Item::Terminal(here(), id)
}

fn gathered_shell(id: u128) -> Item {
    Item::Gathered(Box::new(shell(id)))
}

/// Every worktree of `shelled()` unfolded.
fn unfolded() -> Folds {
    opened(&[worktree("/api"), worktree(DEV), worktree(OAUTH)])
}

#[test]
fn each_shell_has_a_row_after_the_sessions_of_its_worktree() {
    let got = rows(&shelled(), &unfolded());
    assert_eq!(
        got,
        [
            vec![ws("/api")],
            gathered("/api", &["a", "b", "o"], &[1, 2, 3]),
            vec![worktree("/api")],
            group("/api", &[session("a"), session("b")], &[shell(1)]),
            vec![worktree(DEV)],
            group(DEV, &[], &[shell(2), shell(3)]),
            vec![worktree(OAUTH)],
            group(OAUTH, &[session("o")], &[]),
            vec![ws("/notes")],
            group("/notes", &[], &[shell(5)]),
            vec![ws("/tmp")],
            group("/tmp", &[session("z")], &[shell(4)]),
            vec![ws("/web")],
            group("/web", &[session("c")], &[]),
        ]
        .concat()
    );
}

#[test]
fn folding_a_worktree_or_a_workspace_hides_its_shells() {
    let t = shelled();
    let folded = |row: Item| {
        let collapsed = closed(&[row]);
        rows(&t, &collapsed)
    };
    let list = folded(worktree(DEV));
    assert!(list.contains(&worktree(DEV)) && !list.contains(&shell(2)));
    assert!(!folded(ws("/tmp")).contains(&shell(4)));
}

#[test]
fn a_filter_keeps_the_shells_whose_names_match_as_it_keeps_sessions() {
    let t = shelled();
    let list = rows(&narrowed(&t, "log"), &Folds::default());
    assert_eq!(
        list,
        [vec![ws("/notes")], group("/notes", &[], &[shell(5)])].concat()
    );
    // A worktree named by the filter keeps every shell in it.
    let dev = rows(&narrowed(&t, "dev"), &Folds::default());
    assert_eq!(
        dev,
        [
            vec![ws("/api")],
            gathered("/api", &[], &[2, 3]),
            vec![worktree(DEV)],
        ]
        .concat()
    );
    let named = rows(&narrowed(&t, "shell-2"), &Folds::default());
    assert_eq!(
        named,
        [
            vec![ws("/api")],
            gathered("/api", &[], &[3]),
            vec![worktree(DEV)],
        ]
        .concat()
    );
}

#[test]
fn a_shell_tab_selects_the_shell_s_row_or_the_nearest_one_shown() {
    let t = shelled();
    let tab = Tab::Terminal(here(), 2);
    let row = |folds: &Folds| {
        let list = rows(&t, folds);
        row_of(&list, &t, &here(), Some(&tab), &at(DEV))
    };
    assert_eq!(row(&Folds::default()), Some(gathered_shell(2)));
    let across_folded = closed(&[across("/api", Heading::Terminals)]);
    assert_eq!(
        row(&across_folded),
        Some(across("/api", Heading::Terminals))
    );
    let mut folds = unfolded();
    folds.set(&across("/api", Heading::Terminals), false);
    assert_eq!(row(&folds), Some(shell(2)));
    folds.set(&worktree(DEV), false);
    assert_eq!(row(&folds), Some(across("/api", Heading::Terminals)));
}

#[test]
fn the_selection_moves_between_a_session_s_row_and_a_shell_s_with_the_tab() {
    let t = shelled();
    let list = rows(&t, &Folds::default());
    let a = Tab::Session(here(), "a".into());
    let one = Tab::Terminal(here(), 1);
    let switch = |selected: Item, before: &Tab, now: &Tab| {
        row_after_switch(&list, &t, Some(&selected), Some(before), now, &at("/api"))
    };
    assert_eq!(
        switch(gathered_session("a"), &a, &one),
        Some(gathered_shell(1))
    );
    assert_eq!(
        switch(gathered_shell(1), &one, &a),
        Some(gathered_session("a"))
    );
    // A shell gone, its row with it, gives way to the tab shown next.
    assert_eq!(
        switch(shell(9), &Tab::Terminal(here(), 9), &a),
        Some(gathered_session("a"))
    );
    assert_eq!(switch(ws("/web"), &one, &a), None);
}

#[test]
fn a_selected_shell_that_ends_gives_way_to_the_shown_tab_s_row() {
    let t = shelled();
    let list = rows(&t, &Folds::default());
    let shown = Tab::Session(here(), "a".into());
    let kept = row_kept(&list, &t, Some(&shell(9)), Some(&shown), &at("/api"));
    assert_eq!(kept, Some(gathered_session("a")));
}

#[test]
fn a_shell_opens_the_directory_it_runs_in() {
    let t = shelled();
    assert_eq!(place_of(&t, &shell(2)), Some(at(DEV)));
    assert_eq!(place_of(&t, &shell(5)), Some(at("/notes")));
    assert_eq!(place_of(&t, &shell(9)), None);
}

#[test]
fn a_neovim_on_no_file_sits_under_shells_and_one_on_a_file_has_no_row_of_its_own() {
    let mut nvim = sh(6, "nvim", DEV);
    nvim.program = TerminalProgram::Neovim;
    let on_file = TerminalSession {
        cwd: DEV.into(),
        ..editing(7, &format!("{DEV}/a.rs"))
    };
    let t = shelled().with_terminals(&[sh(2, "shell", DEV), nvim, on_file]);
    let w = t.worktrees().find(|w| w.worktree.path == Path::new(DEV));
    let w = w.unwrap();
    assert_eq!(under(&here(), w, Heading::Terminals), [shell(2), shell(6)]);
    let list = rows(&t, &unfolded());
    assert_eq!(row_above(&list, &shell(6)), Some(shell(2)));
    assert!(!list.contains(&shell(7)));
}

#[test]
fn a_neovim_on_a_file_selects_the_file_s_row_or_while_folded_away_the_files_heading() {
    let t = tree().with_terminals(&[TerminalSession {
        cwd: "/web".into(),
        ..editing(9, "/web/README.md")
    }]);
    let nvim = Tab::Terminal(here(), 9);
    let folded = items(&here(), &t, &Folds::default(), &read_web, &BTreeMap::new());
    assert_eq!(
        row_of(&folded, &t, &here(), Some(&nvim), &at("/web")),
        Some(files_of("/web"))
    );
    let mut folds = Folds::default();
    folds.toggle(&files_of("/web"));
    let open = items(&here(), &t, &folds, &read_web, &BTreeMap::new());
    assert_eq!(
        row_of(&open, &t, &here(), Some(&nvim), &at("/web")),
        Some(file("/web", "/web/README.md", false))
    );
}

#[test]
fn a_command_sits_under_shells_in_the_order_started() {
    let mut claude = sh(7, "claude", DEV);
    claude.program = TerminalProgram::Command;
    let t = shelled().with_terminals(&[sh(2, "shell", DEV), claude]);
    let w = t.worktrees().find(|w| w.worktree.path == Path::new(DEV));
    let w = w.unwrap();
    assert_eq!(under(&here(), w, Heading::Terminals), [shell(2), shell(7)]);
    let list = rows(&t, &unfolded());
    assert_eq!(row_above(&list, &shell(7)), Some(shell(2)));
}

#[test]
fn folding_a_heading_hides_its_rows_and_leaves_the_other_headings() {
    let mut folds = unfolded();
    folds.set(&shells_of(DEV), false);
    let list = rows(&shelled(), &folds);
    assert!(list.contains(&shells_of(DEV)) && list.contains(&sessions_of(DEV)));
    assert!(!list.contains(&shell(2)));
}

#[test]
fn a_fold_toggled_twice_is_open_again_and_a_row_that_does_not_fold_is_never_open() {
    let mut folds = Folds::default();
    assert!(folds.is_open(&sessions_of("/api")));
    folds.toggle(&sessions_of("/api"));
    assert!(!folds.is_open(&sessions_of("/api")));
    folds.toggle(&sessions_of("/api"));
    assert!(folds.is_open(&sessions_of("/api")));
    folds.set(&Item::Terminal(here(), 3), true);
    assert!(!folds.is_open(&Item::Terminal(here(), 3)));
}

#[test]
fn a_flat_workspace_holds_its_headings_one_level_higher_than_a_worktree_does() {
    let t = shelled();
    assert_eq!(depth(&t, &ws("/api")), 0);
    assert_eq!(depth(&t, &worktree(DEV)), 1);
    assert_eq!(depth(&t, &shells_of(DEV)), 2);
    assert_eq!(depth(&t, &shell(2)), 3);
    assert_eq!(depth(&t, &shells_of("/tmp")), 1);
    assert_eq!(depth(&t, &shell(4)), 2);
    assert_eq!(depth(&t, &session("z")), 2);
}

#[test]
fn an_opened_session_sits_as_deep_as_any_session_where_it_runs() {
    let t = opening_tree(true);
    assert_eq!(depth(&t, &session("a")), 3);
    assert_eq!(depth(&t, &session("o")), 3);
    assert_eq!(depth(&t, &session("w")), 2);
}

#[test]
fn a_closed_row_gives_way_to_the_one_above_it() {
    let list = rows(&shelled(), &unfolded());
    assert_eq!(
        row_above(&list, &gathered_shell(1)),
        Some(across("/api", Heading::Terminals))
    );
    assert_eq!(row_above(&list, &shell(3)), Some(shell(2)));
    assert_eq!(row_above(&list, &shell(2)), Some(shells_of(DEV)));
    assert_eq!(row_above(&list, &ws("/api")), None);
    assert_eq!(row_above(&list, &shell(9)), None);
}

#[test]
fn shells_are_passed_over_going_from_session_to_session() {
    let t = shelled();
    let shown = Some(Tab::Session(here(), "b".into()));
    let next = next_session(&t, shown.as_ref(), None, true);
    assert_eq!(next.as_deref(), Some("o"));
}

fn listed(id: &str, session_id: &str) -> Session {
    testing::session(id).session_id(session_id).build()
}

#[test]
fn tabs_kept_for_sessions_go_with_them() {
    let sessions = [listed("a08bbcf2", "a08bbcf2-2cbf"), listed("b", "b")];
    let kept: HashSet<Tab> = [
        Tab::Session(here(), "a08bbcf2".into()),
        Tab::Session(here(), "b".into()),
        Tab::Session(here(), "gone".into()),
        Tab::Terminal(here(), 1),
    ]
    .into();
    let expected: HashSet<Tab> = [
        Tab::Session(here(), "a08bbcf2".into()),
        Tab::Session(here(), "b".into()),
        Tab::Terminal(here(), 1),
    ]
    .into();
    assert_eq!(followed(&sessions, &kept), expected);
}
fn editing(id: u128, path: &str) -> TerminalSession {
    TerminalSession {
        program: TerminalProgram::Neovim,
        file: Some(path.into()),
        ..sh(id, "nvim", "/api")
    }
}

#[test]
fn a_session_s_row_opens_the_session_in_its_worktree() {
    let t = tree();
    assert_eq!(
        opens(&session("o"), &t, &[], true),
        Opens::Shows(Tab::Session(here(), "o".into()), Some(at(OAUTH)))
    );
    assert_eq!(
        opens(&Item::Waiting(here(), "b".into()), &t, &[], true),
        Opens::Shows(Tab::Session(here(), "b".into()), Some(at("/api")))
    );
}

#[test]
fn a_worktree_s_row_opens_its_first_session_or_else_a_new_one() {
    let t = tree();
    assert_eq!(
        opens(&worktree(OAUTH), &t, &[], true),
        Opens::Shows(Tab::Session(here(), "o".into()), Some(at(OAUTH)))
    );
    assert_eq!(
        opens(&worktree("/none"), &t, &[], true),
        Opens::Draft("/none".into())
    );
}

#[test]
fn a_file_opens_in_its_viewer_or_in_the_neovim_already_on_it_or_a_new_one() {
    let t = tree();
    let nvim = [editing(9, "/api/a.rs")];
    assert_eq!(
        opens(&file("/api", "/api/logo.png", false), &t, &nvim, true),
        Opens::Shows(
            Tab::Viewer(here(), "/api/logo.png".into()),
            Some(at("/api"))
        )
    );
    assert_eq!(
        opens(&file("/api", "/api/a.rs", false), &t, &nvim, true),
        Opens::Shows(Tab::Terminal(here(), 9), Some(at("/api")))
    );
    assert_eq!(
        opens(&file("/api", "/api/b.rs", false), &t, &nvim, true),
        Opens::Edit("/api".into(), "/api/b.rs".into())
    );
}

#[test]
fn a_document_opens_in_its_viewer_unless_a_neovim_on_it_runs() {
    let t = tree();
    let nvim = [editing(9, "/api/NOTES.md")];
    assert_eq!(
        opens(&file("/api", "/api/README.md", false), &t, &nvim, true),
        Opens::Shows(
            Tab::Viewer(here(), "/api/README.md".into()),
            Some(at("/api"))
        )
    );
    assert_eq!(
        opens(&file("/api", "/api/NOTES.md", false), &t, &nvim, true),
        Opens::Shows(Tab::Terminal(here(), 9), Some(at("/api")))
    );
    assert_eq!(
        opens(&file("/api", "/api/README.md", false), &t, &nvim, false),
        Opens::Edit("/api".into(), "/api/README.md".into())
    );
}

#[test]
fn a_work_machine_s_file_opens_in_neovim_when_it_is_text_and_nowhere_when_not() {
    let t = tree();
    let nvim = [editing(9, "/api/page.html")];
    assert_eq!(
        opens(&file("/api", "/api/page.html", false), &t, &nvim, false),
        Opens::Shows(Tab::Terminal(here(), 9), Some(at("/api")))
    );
    assert_eq!(
        opens(&file("/api", "/api/logo.svg", false), &t, &nvim, false),
        Opens::Edit("/api".into(), "/api/logo.svg".into())
    );
    for path in ["/api/logo.png", "/api/clip.mp4", "/api/paper.pdf"] {
        assert_eq!(
            opens(&file("/api", path, false), &t, &nvim, false),
            Opens::Unavailable,
            "{path}"
        );
    }
    assert_eq!(
        opens(&file("/api", "/api/logo.svg", false), &t, &nvim, true),
        Opens::Shows(
            Tab::Viewer(here(), "/api/logo.svg".into()),
            Some(at("/api"))
        )
    );
}

#[test]
fn a_workspace_a_heading_and_a_directory_fold_rather_than_open() {
    let t = tree();
    let dir = Item::File {
        environment: here(),
        worktree: "/api".into(),
        path: "/api/src".into(),
        dir: true,
    };
    for item in [ws("/api"), sessions_of("/api"), dir] {
        assert_eq!(opens(&item, &t, &[], true), Opens::Fold, "{item:?}");
    }
}

#[test]
fn a_folded_row_unfolds_before_it_opens_whatever_kind_it_is() {
    let folding = [
        ws("/api"),
        worktree(OAUTH),
        sessions_of("/api"),
        files_of("/api"),
        file("/api", "/api/src", true),
    ];
    let mut folds = Folds::default();
    for item in &folding {
        folds.set(item, false);
    }
    for item in &folding {
        assert!(unfolds(item, &folds), "{item:?}");
    }
}

#[test]
fn an_unfolded_row_or_one_that_does_not_fold_opens_as_it_always_does() {
    let mut folds = Folds::default();
    folds.set(&files_of("/api"), true);
    folds.set(&worktree(OAUTH), true);
    folds.set(&session("o"), false);
    for item in [
        ws("/api"),
        worktree(OAUTH),
        sessions_of("/api"),
        files_of("/api"),
        session("a"),
        session("o"),
        Item::Waiting(here(), "a".into()),
        Item::Terminal(here(), 3),
        file("/api", "/api/a.rs", false),
    ] {
        assert!(!unfolds(&item, &folds), "{item:?}");
    }
}

#[test]
fn a_session_has_no_rows_to_fold() {
    assert!(!holds(&session("a")));
    assert!(!holds(&Item::Waiting(here(), "a".into())));
    assert!(holds(&worktree(OAUTH)));
}

#[test]
fn a_row_stands_for_what_windows_show_of_it_a_file_for_its_viewer_and_its_neovim() {
    let nvim = [editing(9, "/api/a.rs"), editing(10, "/api/b.rs")];
    assert_eq!(
        stands_for(&file("/api", "/api/a.rs", false), &nvim),
        [
            Tab::Viewer(here(), "/api/a.rs".into()),
            Tab::Terminal(here(), 9)
        ]
    );
    assert_eq!(
        stands_for(&Item::Waiting(here(), "b".into()), &nvim),
        [Tab::Session(here(), "b".into())]
    );
    assert_eq!(
        stands_for(&Item::Terminal(here(), 3), &nvim),
        [Tab::Terminal(here(), 3)]
    );
    assert!(stands_for(&worktree("/api"), &nvim).is_empty());
}

#[test]
fn a_file_is_edited_while_a_neovim_started_on_it_runs_and_not_for_one_started_elsewhere() {
    let mut elsewhere = sh(3, "nvim", "/api");
    elsewhere.program = TerminalProgram::Neovim;
    let running = [editing(9, "/api/a.rs"), elsewhere];
    assert!(edited(&file("/api", "/api/a.rs", false), &running));
    assert!(!edited(&file("/api", "/api/b.rs", false), &running));
    assert!(!edited(&file("/api", "/api", true), &running));
    assert!(!edited(&Item::Terminal(here(), 9), &running));
}

#[test]
fn a_file_or_worktree_row_copies_its_path_and_what_runs_its_name() {
    let t = tree();
    let shells = [sh(3, "build", "/api")];
    assert_eq!(
        copied(&file("/api", "/api/a.rs", false), &t, &shells),
        Some(Copied::Path("/api/a.rs".into()))
    );
    assert_eq!(
        copied(&worktree(OAUTH), &t, &shells),
        Some(Copied::Path(OAUTH.into()))
    );
    assert_eq!(
        copied(&session("o"), &t, &shells),
        Some(Copied::Name("o".into()))
    );
    assert_eq!(
        copied(&Item::Terminal(here(), 3), &t, &shells),
        Some(Copied::Name("build".into()))
    );
    assert_eq!(copied(&sessions_of("/api"), &t, &shells), None);
}

#[test]
fn a_filter_matches_a_name_whose_letters_come_in_its_order_as_the_pickers_do() {
    let t = shelled();
    let logs = [vec![ws("/notes")], group("/notes", &[], &[shell(5)])].concat();
    assert_eq!(rows(&narrowed(&t, "lgs"), &Folds::default()), logs);
    // A lowercase filter ignores case; one with an uppercase letter keeps it.
    assert_eq!(rows(&narrowed(&t, "LOGS"), &Folds::default()), []);
}
