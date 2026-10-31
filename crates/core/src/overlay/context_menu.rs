//! What a right click on a sidebar row, a tab or a link of the details
//! panel's overview offers: the actions its keys would run there.

use adroit_model::{EnvironmentId, Session};

use crate::actions::{Action, Group};
use crate::content::Tab;
use crate::tree::nav::Item;

/// What a menu was opened on.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Target {
    Row(Item),
    Tab(Tab),
    /// Row `at` of the overview's links, and whether it is a file.
    Link {
        at: usize,
        file: bool,
    },
}

/// `action` as a right-click menu offers it, by what its row in the table
/// of actions calls it there.
fn offer(action: Action) -> (Action, &'static str) {
    let label = action.spec().row.map_or(action.spec().name, |r| r.label);
    (action, label)
}

/// The actions offered on `target`, each with what the menu calls it.
/// `session` is the session the row or tab is for, when it is for one;
/// `own` is the window's own environment, which it never removes.
pub fn entries(
    target: &Target,
    session: Option<&Session>,
    own: &EnvironmentId,
) -> Vec<(Action, &'static str)> {
    let here = [Action::SessionNew, Action::ShellOpen, Action::NeovimOpen].map(offer);
    let ending = session.map(|_| offer(Action::SessionStop));
    let rename = session.map(|_| offer(Action::SessionRename));
    // The key says where a row opens, as in fern.
    let opens = [
        Action::CursorGo,
        Action::CursorSplit,
        Action::CursorVsplit,
        Action::CursorTab,
        Action::CursorShow,
    ]
    .map(offer);
    // What is copied, and what going to a row that folds does, depend on
    // the row, so they are named here rather than in the table.
    let copy_path = (Action::CursorCopy, "Copy path");
    let copy_name = (Action::CursorCopy, "Copy name");
    let fold = (Action::CursorGo, "Fold or unfold");
    let close = offer(Action::CursorClose);
    let close_tab = offer(Action::TabClose);
    let stop_terminal = offer(Action::TerminalStop);
    match target {
        Target::Row(Item::Session(_, _) | Item::Waiting(_, _)) => opens
            .into_iter()
            .chain(rename)
            .chain([copy_name])
            .chain(here)
            .chain([close])
            .chain(ending)
            .collect(),
        Target::Row(Item::Terminal(_, _)) => opens
            .into_iter()
            .chain([copy_name])
            .chain(here)
            .chain([close, stop_terminal])
            .collect(),
        Target::Row(Item::File { dir: false, .. }) => opens
            .into_iter()
            .chain([copy_path])
            .chain(here)
            .chain([close])
            .collect(),
        Target::Row(Item::Worktree(_, _)) => {
            opens.into_iter().chain([copy_path]).chain(here).collect()
        }
        Target::Row(Item::Workspace(_, _) | Item::File { dir: true, .. }) => {
            [fold, copy_path].into_iter().chain(here).collect()
        }
        Target::Row(Item::Heading(..)) => [fold].into_iter().chain(here).collect(),
        // An environment is no place to start anything in.
        Target::Row(Item::Environment(environment)) if environment != own => {
            vec![
                fold,
                offer(Action::SettingsOpen),
                offer(Action::EnvironmentRemove),
            ]
        }
        Target::Row(Item::Environment(_)) => vec![fold, offer(Action::SettingsOpen)],
        Target::Tab(Tab::Session(_, _)) => [close_tab]
            .into_iter()
            .chain(rename)
            .chain(ending)
            .collect(),
        Target::Tab(Tab::Terminal(_, _)) => vec![close_tab, stop_terminal],
        Target::Tab(Tab::Draft { .. }) => Vec::new(),
        Target::Tab(Tab::Viewer(_, _) | Tab::Drawn { .. }) => vec![close_tab],
        Target::Link { file, .. } => [Action::CursorGo, Action::CursorCopy]
            .map(offer)
            .into_iter()
            .chain(file.then(|| offer(Action::CursorReveal)))
            .collect(),
    }
}

/// [`entries`] in the groups the menu draws apart, each entry in the group
/// its row in the table of actions puts it in.
pub fn groups(
    target: &Target,
    session: Option<&Session>,
    own: &EnvironmentId,
) -> Vec<Vec<(Action, &'static str)>> {
    let group = |a: &Action| a.spec().row.map_or(Group::Go, |r| r.group);
    let mut out: Vec<Vec<(Action, &'static str)>> = Vec::new();
    for entry in entries(target, session, own) {
        match out.last_mut() {
            Some(g) if g.last().is_some_and(|(a, _)| group(a) == group(&entry.0)) => g.push(entry),
            _ => out.push(vec![entry]),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::here;

    fn grouped(target: Target, s: Option<&Session>) -> Vec<Vec<Action>> {
        groups(&target, s, &here())
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
                for (action, label) in entries(&target, s, &here()) {
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
    const OPENS: [Action; 5] = [
        Action::CursorGo,
        Action::CursorSplit,
        Action::CursorVsplit,
        Action::CursorTab,
        Action::CursorShow,
    ];

    fn session() -> Session {
        adroit_model::testing::session("a").build()
    }

    fn actions(target: Target, s: Option<&Session>) -> Vec<Action> {
        entries(&target, s, &here())
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
            [&[Action::CursorGo, Action::CursorCopy][..], &starts].concat()
        );
        assert_eq!(
            actions(Target::Row(Item::Worktree(here(), "/api".into())), None),
            [&OPENS[..], &[Action::CursorCopy], &starts].concat()
        );
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
    fn a_file_opens_where_its_key_says_copies_its_path_and_closes_its_windows() {
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
                    Action::CursorClose,
                ],
            ]
            .concat()
        );
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

    #[test]
    fn an_environment_s_heading_offers_its_settings_and_removal_but_the_own_one_no_removal() {
        use crate::testing::there;

        let offered = |environment| {
            entries(&Target::Row(Item::Environment(environment)), None, &here())
                .into_iter()
                .map(|(a, _)| a)
                .collect::<Vec<_>>()
        };
        assert_eq!(
            offered(there()),
            [
                Action::CursorGo,
                Action::SettingsOpen,
                Action::EnvironmentRemove
            ]
        );
        assert_eq!(offered(here()), [Action::CursorGo, Action::SettingsOpen]);
    }
}
