use super::*;
use crate::testing::here;

fn grouped(target: Target, s: Option<&Session>) -> Vec<Vec<Action>> {
    groups(&target, s, &here(), Reach::Here)
        .into_iter()
        .map(|g| g.into_iter().map(|(a, _)| a).collect())
        .collect()
}

#[test]
fn every_entry_of_every_menu_shows_an_icon_and_has_a_row_in_the_table() {
    let own = session();
    let targets = [
        Target::Row(Item::Workspace(here(), "/api".into())),
        Target::Row(Item::Worktree(here(), "/api".into())),
        Target::Row(Item::Session(here(), "a".into())),
        Target::Row(Item::Waiting(here(), "a".into())),
        Target::Row(Item::Terminal(here(), 1)),
        Target::Tab(Tab::Session(here(), "a".into())),
        Target::Tab(Tab::Terminal(here(), 1)),
        Target::Link { at: 0, file: true },
    ];
    for target in targets {
        for s in [None, Some(&own)] {
            for (action, label) in entries(&target, s, &here(), Reach::Here) {
                assert!(action.icon().is_some(), "{label}");
                assert!(action.spec().row.is_some(), "{label}");
            }
        }
    }
}

#[test]
fn a_link_of_the_overview_is_opened_or_copied_and_a_file_revealed_too() {
    assert_eq!(
        actions(Target::Link { at: 2, file: false }, None),
        [Action::CursorGo, Action::CursorCopy]
    );
    assert_eq!(
        actions(Target::Link { at: 2, file: true }, None),
        [Action::CursorGo, Action::CursorCopy, Action::CursorReveal]
    );
}

#[test]
fn a_menu_draws_going_starting_and_ending_apart() {
    let s = session();
    assert_eq!(
        grouped(Target::Row(Item::Session(here(), "a".into())), Some(&s)),
        [
            [&OPENS[..], &[Action::SessionRename, Action::CursorCopy]].concat(),
            vec![Action::SessionNew, Action::ShellOpen, Action::NeovimOpen],
            vec![Action::CursorClose, Action::SessionStop],
        ]
    );
    assert_eq!(
        grouped(Target::Tab(Tab::Terminal(here(), 1)), None),
        [vec![Action::TabClose, Action::TerminalStop]]
    );
    let draft = Tab::draft(here(), "/a");
    assert!(grouped(Target::Tab(draft), None).is_empty());
}

/// What every row that opens offers first: where to open it.
const OPENS: [Action; 4] = [
    Action::CursorGo,
    Action::CursorSplit,
    Action::CursorVsplit,
    Action::CursorTab,
];

fn session() -> Session {
    adroit_model::testing::session("a").build()
}

fn actions(target: Target, s: Option<&Session>) -> Vec<Action> {
    entries(&target, s, &here(), Reach::Here)
        .into_iter()
        .map(|(a, _)| a)
        .collect()
}

#[test]
fn a_session_adroit_runs_can_be_opened_renamed_started_beside_and_ended() {
    let s = session();
    assert_eq!(
        actions(Target::Row(Item::Session(here(), "a".into())), Some(&s)),
        [
            &OPENS[..],
            &[
                Action::SessionRename,
                Action::CursorCopy,
                Action::SessionNew,
                Action::ShellOpen,
                Action::NeovimOpen,
                Action::CursorClose,
                Action::SessionStop,
            ],
        ]
        .concat()
    );
}

#[test]
fn a_workspace_or_worktree_starts_sessions_and_shells_in_it() {
    let starts = [Action::SessionNew, Action::ShellOpen, Action::NeovimOpen];
    assert_eq!(
        actions(Target::Row(Item::Workspace(here(), "/api".into())), None),
        [
            &[Action::CursorGo, Action::CursorCopy][..],
            &starts,
            &[Action::WorkspaceRemove]
        ]
        .concat()
    );
    assert_eq!(
        actions(Target::Row(Item::Worktree(here(), "/api".into())), None),
        [
            &OPENS[..],
            &[Action::CursorCopy],
            &starts,
            &[Action::WorktreeClose]
        ]
        .concat()
    );
}

#[test]
fn the_plus_of_an_environment_s_heading_offers_only_what_starts_work_there() {
    let start = Target::Start(Item::Environment(crate::testing::there()));
    assert_eq!(actions(start.clone(), None), STARTS);
    for (action, label) in entries(&start, None, &here(), Reach::Here) {
        assert!(action.icon().is_some(), "{label}");
        assert!(action.spec().row.is_some(), "{label}");
    }
}

#[test]
fn a_terminal_row_opens_it_starts_beside_it_and_offers_an_explicit_stop() {
    assert_eq!(
        actions(Target::Row(Item::Terminal(here(), 1)), None),
        [
            &OPENS[..],
            &[
                Action::CursorCopy,
                Action::SessionNew,
                Action::ShellOpen,
                Action::NeovimOpen,
                Action::CursorClose,
                Action::TerminalStop,
            ],
        ]
        .concat()
    );
}

#[test]
fn a_file_opens_where_its_key_says_copies_its_path_makes_beside_it_and_closes_its_windows() {
    let file = Item::File {
        environment: here(),
        worktree: "/api".into(),
        path: "/api/a.rs".into(),
        dir: false,
    };
    assert_eq!(
        actions(Target::Row(file), None),
        [
            &OPENS[..],
            &[
                Action::CursorCopy,
                Action::SessionNew,
                Action::ShellOpen,
                Action::NeovimOpen,
                Action::FileNew,
                Action::DirectoryNew,
                Action::FileCopy,
                Action::FileMove,
                Action::FilePaste,
                Action::CursorClose,
                Action::FileTrash,
            ],
        ]
        .concat()
    );
}

fn dir_row() -> Item {
    Item::File {
        environment: here(),
        worktree: "/api".into(),
        path: "/api/src".into(),
        dir: true,
    }
}

#[test]
fn a_directory_and_the_files_heading_make_and_paste_files_in_them() {
    let dir = dir_row();
    let files = Item::Heading(here(), "/api".into(), crate::tree::nav::Heading::Files);
    let sessions = Item::Heading(here(), "/api".into(), crate::tree::nav::Heading::Sessions);
    for row in [dir, files] {
        let got = actions(Target::Row(row.clone()), None);
        assert!(
            got.windows(2)
                .any(|w| w == [Action::FileNew, Action::DirectoryNew]),
            "{row:?}"
        );
        assert!(got.contains(&Action::FilePaste), "{row:?}");
    }
    let dir_menu = actions(Target::Row(dir_row()), None);
    assert!(dir_menu.ends_with(&[
        Action::FileCopy,
        Action::FileMove,
        Action::FilePaste,
        Action::FileTrash
    ]));
    assert!(!actions(Target::Row(sessions), None).contains(&Action::FileNew));
}

#[test]
fn a_heading_folds_and_has_nothing_to_copy() {
    let got = actions(
        Target::Row(Item::Heading(
            here(),
            "/api".into(),
            crate::tree::nav::Heading::Files,
        )),
        None,
    );
    assert_eq!(got[0], Action::CursorGo);
    assert!(!got.contains(&Action::CursorCopy));
}

#[test]
fn closing_a_tab_and_stopping_its_agent_or_terminal_are_distinct_actions() {
    let s = session();
    assert_eq!(
        actions(Target::Tab(Tab::Session(here(), "a".into())), Some(&s)),
        [Action::TabClose, Action::SessionRename, Action::SessionStop]
    );
    assert_eq!(
        actions(Target::Tab(Tab::Terminal(here(), 1)), None),
        [Action::TabClose, Action::TerminalStop]
    );
}

fn heading_groups(environment: EnvironmentId, reach: Reach) -> Vec<Vec<Action>> {
    groups(
        &Target::Row(Item::Environment(environment)),
        None,
        &here(),
        reach,
    )
    .into_iter()
    .map(|g| g.into_iter().map(|(a, _)| a).collect())
    .collect()
}

const STARTS: [Action; 4] = [
    Action::SessionNew,
    Action::ShellOpen,
    Action::NeovimOpen,
    Action::WorkspaceAdd,
];
const CONFIGURES: [Action; 3] = [
    Action::SettingsOpen,
    Action::EnvironmentRename,
    Action::CursorGo,
];

#[test]
fn an_environment_s_heading_starts_work_in_it_configures_it_and_removes_it() {
    use crate::testing::there;

    assert_eq!(
        heading_groups(there(), Reach::Here),
        [
            STARTS.to_vec(),
            CONFIGURES.to_vec(),
            vec![Action::EnvironmentRemove],
        ]
    );
}

#[test]
fn the_window_s_own_environment_is_never_offered_for_removal() {
    assert_eq!(
        heading_groups(here(), Reach::Here),
        [STARTS.to_vec(), CONFIGURES.to_vec()]
    );
}

#[test]
fn a_work_machine_s_heading_pairs_again_and_retries_only_while_the_window_waits_to() {
    use crate::testing::there;

    let reaching = |retryable| heading_groups(there(), Reach::Machine { retryable })[2].clone();
    assert_eq!(reaching(false), [Action::EnvironmentPair]);
    assert_eq!(
        reaching(true),
        [Action::EnvironmentRetry, Action::EnvironmentPair]
    );
}

#[test]
fn every_entry_of_a_heading_s_menu_shows_an_icon_and_has_a_row_in_the_table() {
    let target = Target::Row(Item::Environment(crate::testing::there()));
    for (action, label) in entries(&target, None, &here(), Reach::Machine { retryable: true }) {
        assert!(action.icon().is_some(), "{label}");
        assert!(action.spec().row.is_some(), "{label}");
    }
}
