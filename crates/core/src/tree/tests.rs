use crate::testing::here;
use adroit_model::{Session, Wait};
use adroit_protocol::{Sight, TreeChange};

use super::*;
use crate::actions::Action;
use crate::msg::Msg;
use crate::scenario::Scenario;
use crate::testing::{self, listing, patch, session, session_tab, shell, show_in};

fn rows(run: &Scenario) -> Vec<Item> {
    run.state().tree.rows().to_vec()
}

fn selected(run: &Scenario) -> Option<Item> {
    run.state().tree.selected.clone()
}

fn sights(run: &Scenario) -> Vec<TreeSight> {
    run.effects()
        .iter()
        .filter_map(|e| match e {
            Effect::Host {
                request: Request::Watch(sight),
                ..
            } => Some(sight.tree.clone()),
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
            reach: crate::overlay::context_menu::Reach::Here,
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
        let tree = listed(run.state().hosts.own().work.as_ref()).clone();
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
    assert!(got.contains(&Item::Gathered(Box::new(Item::Session(here(), "a".into())))));
    assert_eq!(got.last(), Some(&Item::Waiting(here(), "b".into())));
    assert_eq!(run.state().tree.waiting().len(), 1);
}

#[test]
fn only_the_environment_s_heading_is_listed_before_its_host_lists_anything() {
    let run = Scenario::new().send(Msg::TreeFilter(String::new()));
    assert_eq!(rows(&run), [Item::Environment(here())]);
    assert_eq!(
        listed(run.state().hosts.own().work.as_ref()),
        &Tree::default()
    );
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
    assert!(run.state().tree.narrowed_waiting().is_empty());
    assert_eq!(run.state().tree.waiting().len(), 1);
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
    assert_eq!(rows(&run), [Item::Environment(here()), ws.clone()]);
    let unfolded = run.send(Msg::TreeFoldToggle(ws));
    assert!(rows(&unfolded).contains(&Item::Session(here(), "a".into())));
}

#[test]
fn a_workspace_where_nothing_runs_is_listed_folded() {
    let notes = Item::Workspace(here(), "/notes".into());
    let run = Scenario::new().event(
        testing::snapshot().listed(
            testing::listed()
                .session(session("a").cwd("/r"))
                .kept("/notes"),
        ),
    );
    let after = |run: &Scenario| {
        let got = rows(run);
        let at = got.iter().position(|r| *r == notes).unwrap();
        got.get(at + 1).cloned()
    };
    assert_eq!(after(&run), Some(Item::Workspace(here(), "/r".into())));
    assert!(rows(&run).contains(&Item::Session(here(), "a".into())));
    let unfolded = run.send(Msg::TreeFoldToggle(notes.clone()));
    assert_ne!(after(&unfolded), Some(Item::Workspace(here(), "/r".into())));
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
    let listed = run
        .event(patch(2).listed(
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

/// `/r` on GitHub as `o/r`, with pull request 7 read.
fn on_github() -> TreeChange {
    let mut repository = Repository::new("o/r");
    repository.pulls = Some(Ok(vec![adroit_model::github::Item {
        number: 7,
        title: "Fix it".into(),
        url: "https://github.com/o/r/pull/7".into(),
        author: "me".into(),
        draft: false,
        ..Default::default()
    }]));
    TreeChange::GitHub {
        repository: "/r".into(),
        github: Some(Box::new(repository)),
    }
}

#[test]
fn the_host_keeps_read_the_listings_on_github_the_rows_hold_open() {
    let pulls = Item::GitHub(here(), "/r".into(), Listing::Pulls);
    let run = Scenario::new()
        .event(listing([session("a").cwd("/r")]))
        .event(patch(2).change(on_github()));
    assert!(rows(&run).contains(&pulls));
    assert!(sights(&run).last().is_none_or(|s| s.github.is_empty()));
    let opened = run.send(Msg::TreeFold {
        row: pulls.clone(),
        open: true,
    });
    assert_eq!(
        sights(&opened).last().unwrap().github,
        BTreeSet::from([("/r".into(), Listing::Pulls)])
    );
    let folded = opened.send(Msg::TreeFold {
        row: pulls,
        open: false,
    });
    assert!(sights(&folded).last().unwrap().github.is_empty());
}

#[test]
fn opening_a_pull_request_or_an_issue_shows_it_in_a_tab_of_its_own_in_its_repository() {
    let pull = Item::GitHubItem(here(), "/r".into(), Listing::Pulls, 7);
    for remote in [false, true] {
        let mut run = Scenario::new()
            .event(listing([session("a").cwd("/r")]))
            .event(patch(2).change(on_github()));
        if remote {
            run = run.send(Msg::OnWorkMachine(here()));
        }
        let run = run.send(Msg::OpenRow {
            item: pull.clone(),
            how: None,
            take_keys: true,
        });
        let content = &run.state().content;
        assert_eq!(
            content.active(),
            Some(&Tab::GitHub {
                environment: here(),
                repository: "/r".into(),
                listing: Listing::Pulls,
                number: 7,
            })
        );
        assert_eq!(
            content.place.as_ref().map(|p| p.worktree.clone()),
            Some("/r".into())
        );
        assert_eq!(content.focus, crate::content::Area::Console);
        assert_eq!(selected(&run), Some(pull.clone()));
    }
}

#[test]
fn a_pull_request_s_tab_is_named_by_its_title_where_the_window_shows_it() {
    let run = Scenario::new()
        .event(listing([session("a").cwd("/r")]))
        .event(patch(2).change(on_github()))
        .send(Msg::OpenRow {
            item: Item::GitHubItem(here(), "/r".into(), Listing::Pulls, 7),
            how: None,
            take_keys: true,
        });
    let showing = run.state().content.showing();
    let crumbs = crumbs(&showing, run.state().hosts.work(&here())).unwrap();
    assert_eq!(crumbs.target, "Fix it");
}

#[test]
fn listing_every_worktree_is_asked_of_the_host_and_said() {
    let run = Scenario::new().send(Msg::TreeEveryWorktree);
    assert!(run.last_effects().contains(&Effect::Host {
        environment: crate::testing::here(),
        request: Request::Relist,
    }));
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
        run.state().hosts.own().work.as_ref(),
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
            nothing.state().hosts.own().work.as_ref()
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
    let place = |w: &str| Some(Place::at(&here(), w));
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

/// An environment other than the window's own.
fn there() -> EnvironmentId {
    EnvironmentId::new("there")
}

/// A window showing its own environment and `there`, each with a
/// session in a directory of the same path.
fn two_environments() -> Scenario {
    Scenario::new()
        .send(Msg::EnvironmentOpened {
            environment: there(),
            name: "Work machine".into(),
        })
        .event_from(here(), listing([session("a").cwd("/r")]))
        .event_from(there(), listing([session("b").cwd("/r")]))
}

fn at(rows: &[Item], item: &Item) -> usize {
    rows.iter()
        .position(|r| r == item)
        .unwrap_or_else(|| panic!("{item:?} is not among {rows:?}"))
}

#[test]
fn a_window_showing_one_environment_heads_it_too() {
    let run = Scenario::new().event(listing([session("a").cwd("/r")]));
    let got = rows(&run);
    let ws = Item::Workspace(here(), "/r".into());
    assert_eq!(got[..2], [Item::Environment(here()), ws.clone()]);
    let view = &run.state().tree;
    assert_eq!(view.depth(&got[0]), 0);
    assert_eq!(view.depth(&ws), 1);
    assert_eq!(view.parent(&ws), Some(Item::Environment(here())));
}

#[test]
fn each_environment_s_work_sits_under_a_heading_of_its_own() {
    let run = two_environments();
    let got = rows(&run);
    let own = Item::Environment(here());
    let other = Item::Environment(there());
    assert_eq!(got.first(), Some(&own));
    assert!(at(&got, &Item::Session(here(), "a".into())) < at(&got, &other));
    assert!(at(&got, &other) < at(&got, &Item::Workspace(there(), "/r".into())));
    assert!(
        at(&got, &Item::Workspace(there(), "/r".into()))
            < at(&got, &Item::Session(there(), "b".into()))
    );
    let view = &run.state().tree;
    assert_eq!(view.depth(&other), 0);
    assert_eq!(view.depth(&Item::Workspace(there(), "/r".into())), 1);
    assert_eq!(
        view.parent(&Item::Workspace(there(), "/r".into())),
        Some(other)
    );
}

#[test]
fn the_same_directory_in_two_environments_is_two_rows() {
    let got = rows(&two_environments());
    let workspaces: Vec<_> = got
        .iter()
        .filter(|r| matches!(r, Item::Workspace(_, root) if root == Path::new("/r")))
        .collect();
    assert_eq!(
        workspaces,
        [
            &Item::Workspace(here(), "/r".into()),
            &Item::Workspace(there(), "/r".into())
        ]
    );
}

#[test]
fn an_environment_heading_is_named_by_the_name_it_was_opened_with_or_else_its_id() {
    let run = two_environments().event_from(
        EnvironmentId::new("elsewhere"),
        listing(Vec::<Session>::new()),
    );
    let hosts = &run.state().hosts;
    assert_eq!(hosts.name(&there()), "Work machine");
    assert_eq!(hosts.name(&EnvironmentId::new("elsewhere")), "elsewhere");
    assert!(!hosts.name(&here()).is_empty());
}

#[test]
fn folding_one_environment_leaves_the_other_open() {
    let folded = two_environments().send(Msg::TreeFold {
        row: Item::Environment(there()),
        open: false,
    });
    let got = rows(&folded);
    assert!(got.contains(&Item::Environment(there())));
    assert!(!got.contains(&Item::Workspace(there(), "/r".into())));
    assert!(got.contains(&Item::Session(here(), "a".into())));
    let unfolded = folded.send(Msg::TreeFoldToggle(Item::Environment(there())));
    assert!(rows(&unfolded).contains(&Item::Session(there(), "b".into())));
}

#[test]
fn the_fold_key_on_an_environment_s_top_row_folds_the_environment() {
    let ws = Item::Workspace(there(), "/r".into());
    let run = two_environments()
        .send(Msg::TreeFold {
            row: ws.clone(),
            open: false,
        })
        .send(Msg::TreeSelect(Some(ws)))
        .send(Msg::TreeKey(Key::Fold));
    assert_eq!(selected(&run), Some(Item::Environment(there())));
    assert!(!run.state().tree.folds.is_open(&Item::Environment(there())));
    assert!(rows(&run).contains(&Item::Session(here(), "a".into())));
}

fn both_waiting() -> Scenario {
    Scenario::new()
        .event_from(
            here(),
            listing([session("a").cwd("/r").waiting(Wait::Permission)]),
        )
        .event_from(
            there(),
            listing([session("b").cwd("/r").waiting(Wait::Permission)]),
        )
}

#[test]
fn what_waits_on_the_user_is_gathered_from_every_environment() {
    let run = both_waiting();
    let waiting: Vec<_> = run
        .state()
        .tree
        .waiting()
        .iter()
        .map(|(environment, it)| (environment.clone(), it.session.id.clone()))
        .collect();
    assert_eq!(
        waiting,
        [(here(), "a".to_string()), (there(), "b".to_string())]
    );
    let got = rows(&run);
    assert_eq!(
        got[got.len() - 2..],
        [
            Item::Waiting(here(), "a".into()),
            Item::Waiting(there(), "b".into())
        ]
    );
}

#[test]
fn the_sessions_waiting_on_the_user_are_counted_by_environment() {
    let run = both_waiting().event_from(
        there(),
        patch(2).listed(testing::listed().sessions([
            session("b").cwd("/r").waiting(Wait::Permission),
            session("c").cwd("/r").waiting(Wait::Permission),
        ])),
    );
    let view = &run.state().tree;
    assert_eq!(view.waiting_in(&here()), 1);
    assert_eq!(view.waiting_in(&there()), 2);
    assert_eq!(view.waiting_in(&EnvironmentId::new("elsewhere")), 0);
}

#[test]
fn the_next_waiting_session_can_be_another_environment_s() {
    use crate::console::Probe;
    let next = |run: Scenario| {
        run.send(Msg::Perform {
            action: Action::WaitingNext,
            key: None,
            probe: Box::new(Probe::default()),
        })
    };
    let run = next(both_waiting().send(Msg::TreeSelect(Some(Item::Waiting(here(), "a".into())))));
    assert_eq!(selected(&run), Some(Item::Waiting(there(), "b".into())));
    assert_eq!(
        run.state().content.active(),
        Some(&Tab::Session(there(), "b".into()))
    );
    let back = next(run);
    assert_eq!(selected(&back), Some(Item::Waiting(here(), "a".into())));
}

#[test]
fn relisting_is_asked_of_every_environment_s_host() {
    for msg in [Msg::Relist, Msg::TreeEveryWorktree] {
        let run = two_environments().send(msg.clone());
        for environment in [here(), there()] {
            assert!(
                run.last_effects().contains(&Effect::Host {
                    environment: environment.clone(),
                    request: Request::Relist,
                }),
                "{msg:?} {environment}"
            );
        }
    }
}

#[test]
fn each_environment_s_host_keeps_read_the_files_of_its_own_rows() {
    let watched = |run: &Scenario, environment: EnvironmentId| {
        run.effects()
            .iter()
            .filter_map(|e| match e {
                Effect::Host {
                    environment: to,
                    request: Request::Watch(sight),
                } if *to == environment => Some(sight.tree.clone()),
                _ => None,
            })
            .next_back()
    };
    let run = two_environments().send(Msg::TreeFold {
        row: Item::Heading(there(), "/r".into(), Heading::Files),
        open: true,
    });
    let there_sight = watched(&run, there()).unwrap();
    assert_eq!(there_sight.marked, BTreeSet::from(["/r".into()]));
    assert!(watched(&run, here()).is_none_or(|s| s.marked.is_empty()));
}

#[test]
fn the_host_of_the_environment_the_tree_is_at_keeps_the_details_read() {
    let details = |run: &Scenario, environment: EnvironmentId| {
        run.effects()
            .iter()
            .filter_map(|e| match e {
                Effect::Host {
                    environment: to,
                    request: Request::Watch(sight),
                } if *to == environment => Some(sight.details),
                _ => None,
            })
            .next_back()
            .unwrap_or(false)
    };
    let run = two_environments();
    let run = if run.state().details.shown.open {
        run
    } else {
        run.send(Msg::PanelToggle(
            crate::chrome::panels::layout::Side::Details,
        ))
    };
    assert!(run.state().details.shown.open);
    let run = run.send(Msg::TreeSelect(Some(Item::Environment(there()))));
    assert!(details(&run, there()));
    let within = run.send(Msg::TreeSelect(Some(Item::Workspace(there(), "/r".into()))));
    assert!(details(&within, there()));
    let away = within.send(Msg::TreeSelect(Some(Item::Workspace(here(), "/r".into()))));
    assert!(!details(&away, there()));
}

#[test]
fn only_the_host_of_the_environment_whose_heading_is_selected_lists_its_agents() {
    let agents = |run: &Scenario, environment: EnvironmentId| {
        crate::settle::sight(run.state(), &environment).agents
    };
    let heading = |environment: EnvironmentId| {
        let run = two_environments();
        let run = if run.state().details.shown.open {
            run
        } else {
            run.send(Msg::PanelToggle(
                crate::chrome::panels::layout::Side::Details,
            ))
        };
        // On the tab that lists the profiles, as Adroit has it.
        run.send(Msg::TreeSelect(Some(Item::Environment(environment))))
            .send(Msg::DetailsTab(1))
    };
    let there_heading = heading(there());
    let overview = heading(there()).send(Msg::DetailsTab(0));
    assert!(!agents(&overview, there()), "the profiles are not in sight");
    assert!(agents(&there_heading, there()));
    assert!(!agents(&there_heading, here()));
    let here_heading = heading(here());
    assert!(agents(&here_heading, here()));
    assert!(!agents(&here_heading, there()));
    let within = there_heading.send(Msg::TreeSelect(Some(Item::Workspace(there(), "/r".into()))));
    assert!(!agents(&within, there()));
    let closed = heading(there()).send(Msg::PanelToggle(
        crate::chrome::panels::layout::Side::Details,
    ));
    assert!(!closed.state().details.shown.open);
    assert!(!agents(&closed, there()));
}

#[test]
fn the_environment_at_hand_is_the_selected_row_s_or_else_the_one_shown() {
    let run = two_environments();
    assert_eq!(run.state().tree.environment_at(&here()), here());
    let selected = run.send(Msg::TreeSelect(Some(Item::Environment(there()))));
    assert_eq!(selected.state().tree.environment_at(&here()), there());
}

fn perform(action: Action) -> Msg {
    Msg::Perform {
        action,
        key: None,
        probe: Box::new(crate::console::Probe::default()),
    }
}

/// `with_src` with `/r/src` selected, a new entry of `action` being
/// named in it.
fn naming_in_src(action: Action) -> (Scenario, Item) {
    let (run, src) = with_src();
    let run = run
        .send(Msg::Focus(crate::content::Area::Sidebar))
        .send(Msg::TreeSelect(Some(src.clone())))
        .send(perform(action));
    (run, src)
}

#[test]
fn a_new_file_is_named_under_the_directory_selected_which_opens_for_it() {
    use crate::overlay::naming::Kind;
    let (run, src) = naming_in_src(Action::FileNew);
    let naming = run.state().overlays.naming.clone().expect("a name asked");
    assert_eq!(naming.dir, PathBuf::from("/r/src"));
    assert_eq!(naming.kind, Kind::File);
    assert!(run.state().tree.folds.is_open(&src));
    let (run, _) = naming_in_src(Action::DirectoryNew);
    let naming = run.state().overlays.naming.clone().expect("a name asked");
    assert_eq!(naming.kind, Kind::Directory);
}

#[test]
fn off_the_files_no_name_is_asked() {
    let (run, _) = with_src();
    let run = run
        .send(Msg::Focus(crate::content::Area::Sidebar))
        .send(Msg::TreeSelect(Some(Item::Session(here(), "a".into()))))
        .send(perform(Action::FileNew));
    assert_eq!(run.state().overlays.naming, None);
}

#[test]
fn the_name_typed_asks_the_host_to_make_it() {
    let (run, _) = naming_in_src(Action::FileNew);
    let run = run
        .send(Msg::NamingText("a/b.rs".into()))
        .send(Msg::NamingEnd { commit: true });
    assert_eq!(run.state().overlays.naming, None);
    assert_eq!(
        run.last_effects(),
        [Effect::MakeEntry {
            environment: here(),
            worktree: "/r".into(),
            path: "/r/src/a/b.rs".into(),
            dir: false,
        }]
    );
}

#[test]
fn a_name_given_up_or_empty_makes_nothing() {
    for (text, commit) in [("a.rs", false), ("  ", true)] {
        let (run, _) = naming_in_src(Action::FileNew);
        let run = run
            .send(Msg::NamingText(text.into()))
            .send(Msg::NamingEnd { commit });
        assert_eq!(run.state().overlays.naming, None, "{text}");
        assert!(
            !run.last_effects()
                .iter()
                .any(|e| matches!(e, Effect::MakeEntry { .. })),
            "{text}"
        );
    }
}

#[test]
fn a_name_reaching_outside_its_directory_is_told_and_makes_nothing() {
    let (run, _) = naming_in_src(Action::FileNew);
    let run = run
        .send(Msg::NamingText("../x".into()))
        .send(Msg::NamingEnd { commit: true });
    assert_eq!(run.state().overlays.naming, None);
    assert!(
        !run.last_effects()
            .iter()
            .any(|e| matches!(e, Effect::MakeEntry { .. }))
    );
    assert!(
        run.state()
            .chrome
            .status
            .line
            .as_deref()
            .is_some_and(|t| t.contains("../x"))
    );
}

#[test]
fn the_field_goes_when_its_directory_is_no_longer_listed() {
    let (run, src) = naming_in_src(Action::FileNew);
    let run = run.send(Msg::TreeFold {
        row: Item::Heading(here(), "/r".into(), Heading::Files),
        open: false,
    });
    assert!(!run.state().tree.rows().contains(&src));
    assert_eq!(run.state().overlays.naming, None);
}

#[test]
fn what_is_made_is_selected_once_listed_its_directories_open() {
    let (run, src) = with_src();
    let a = Item::File {
        environment: here(),
        worktree: "/r".into(),
        path: "/r/src/a".into(),
        dir: true,
    };
    let b = Item::File {
        environment: here(),
        worktree: "/r".into(),
        path: "/r/src/a/b.rs".into(),
        dir: false,
    };
    let run = run
        .send(Msg::TreeSelect(Some(src.clone())))
        .send(Msg::EntryMade {
            environment: here(),
            worktree: "/r".into(),
            path: "/r/src/a/b.rs".into(),
            dir: false,
        });
    assert!(run.state().tree.folds.is_open(&src));
    assert!(run.state().tree.folds.is_open(&a));
    assert_eq!(selected(&run), Some(src.clone()));
    let run = run
        .event(patch(3).change(TreeChange::Entries {
            dir: "/r/src".into(),
            entries: vec![Entry {
                path: "/r/src/a".into(),
                dir: true,
            }],
        }))
        .event(patch(4).change(TreeChange::Entries {
            dir: "/r/src/a".into(),
            entries: vec![Entry {
                path: "/r/src/a/b.rs".into(),
                dir: false,
            }],
        }));
    assert_eq!(selected(&run), Some(b));
}

#[test]
fn what_is_made_is_not_selected_once_the_selection_has_moved_on() {
    let (run, src) = with_src();
    let run = run
        .send(Msg::TreeSelect(Some(src.clone())))
        .send(Msg::EntryMade {
            environment: here(),
            worktree: "/r".into(),
            path: "/r/src/lib.rs".into(),
            dir: false,
        })
        .send(Msg::TreeSelect(Some(Item::Session(here(), "a".into()))))
        .event(patch(3).change(TreeChange::Entries {
            dir: "/r/src".into(),
            entries: vec![Entry {
                path: "/r/src/lib.rs".into(),
                dir: false,
            }],
        }));
    assert_eq!(selected(&run), Some(Item::Session(here(), "a".into())));
}
