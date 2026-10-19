//! What a right click on a sidebar row or a tab offers: the actions its keys
//! would run there.

use adroit_model::{Kind, Session};

use crate::keymap::Action;
use crate::nav::Item;
use crate::tabs::Tab;

/// What a menu was opened on.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Target {
    Row(Item),
    Tab(Tab),
}

/// The actions offered on `target`, each with what the menu calls it.
/// `session` is the session the row or tab is for, when it is for one.
pub fn entries(target: &Target, session: Option<&Session>) -> Vec<(Action, &'static str)> {
    let here = [
        (Action::SessionNew, "New session here"),
        (Action::SessionNewReader, "New reader here"),
        (Action::ShellOpen, "Open a shell here"),
    ];
    let ending = session.map(|s| match s.kind {
        Kind::Interactive => (Action::SessionTakeOver, "Take over…"),
        Kind::Own => (Action::SessionStop, "End…"),
    });
    // A session in another terminal is named there.
    let rename = session
        .filter(|s| s.kind == Kind::Own)
        .map(|_| (Action::SessionRename, "Rename…"));
    match target {
        Target::Row(Item::Session(_) | Item::Waiting(_)) => {
            [(Action::CursorOpen, "Open"), (Action::CursorShow, "Show")]
                .into_iter()
                .chain(rename)
                .chain(here)
                .chain(ending)
                .collect()
        }
        Target::Row(Item::Shell(_)) => [(Action::CursorOpen, "Open"), (Action::CursorShow, "Show")]
            .into_iter()
            .chain(here)
            .chain([(Action::TabClose, "Close shell")])
            .collect(),
        Target::Row(Item::Worktree(_)) => {
            [(Action::CursorOpen, "Open"), (Action::CursorShow, "Show")]
                .into_iter()
                .chain(here)
                .collect()
        }
        Target::Row(Item::Workspace(_)) => [(Action::CursorOpen, "Fold or unfold")]
            .into_iter()
            .chain(here)
            .collect(),
        Target::Tab(Tab::Session(_)) => [(Action::TabClose, "Close tab")]
            .into_iter()
            .chain(rename)
            .chain(ending)
            .collect(),
        Target::Tab(Tab::Shell(_)) => vec![(Action::TabClose, "Close shell")],
        Target::Tab(Tab::Draft { .. }) => Vec::new(),
    }
}

/// [`entries`] in the groups the menu draws apart: going to what was
/// clicked, starting something beside it, and ending or closing it.
pub fn groups(target: &Target, session: Option<&Session>) -> Vec<Vec<(Action, &'static str)>> {
    let kin = |a: &Action| match a {
        Action::SessionNew | Action::SessionNewReader | Action::ShellOpen => 1,
        Action::SessionTakeOver | Action::SessionStop | Action::TabClose => 2,
        _ => 0,
    };
    let mut out: Vec<Vec<(Action, &'static str)>> = Vec::new();
    for entry in entries(target, session) {
        match out.last_mut() {
            Some(group) if group.last().is_some_and(|(a, _)| kin(a) == kin(&entry.0)) => {
                group.push(entry)
            }
            _ => out.push(vec![entry]),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn grouped(target: Target, s: Option<&Session>) -> Vec<Vec<Action>> {
        groups(&target, s)
            .into_iter()
            .map(|g| g.into_iter().map(|(a, _)| a).collect())
            .collect()
    }

    #[test]
    fn every_entry_of_every_menu_shows_an_icon() {
        let own = session(Kind::Own);
        let elsewhere = session(Kind::Interactive);
        let targets = [
            Target::Row(Item::Workspace("/api".into())),
            Target::Row(Item::Worktree("/api".into())),
            Target::Row(Item::Session("a".into())),
            Target::Row(Item::Waiting("a".into())),
            Target::Row(Item::Shell(1)),
            Target::Tab(Tab::Session("a".into())),
            Target::Tab(Tab::Shell(1)),
        ];
        for target in targets {
            for s in [None, Some(&own), Some(&elsewhere)] {
                for (action, label) in entries(&target, s) {
                    assert!(crate::icons::Icon::of(&action).is_some(), "{label}");
                }
            }
        }
    }

    #[test]
    fn a_menu_draws_going_starting_and_ending_apart() {
        let s = session(Kind::Own);
        assert_eq!(
            grouped(Target::Row(Item::Session("a".into())), Some(&s)),
            [
                vec![
                    Action::CursorOpen,
                    Action::CursorShow,
                    Action::SessionRename
                ],
                vec![
                    Action::SessionNew,
                    Action::SessionNewReader,
                    Action::ShellOpen
                ],
                vec![Action::SessionStop],
            ]
        );
        assert_eq!(
            grouped(Target::Tab(Tab::Shell(1)), None),
            [vec![Action::TabClose]]
        );
        let draft = Tab::Draft {
            worktree: "/a".into(),
            role: crate::tabs::Role::Worker,
        };
        assert!(grouped(Target::Tab(draft), None).is_empty());
    }

    fn session(kind: Kind) -> Session {
        Session {
            id: "a".into(),
            kind,
            ..Default::default()
        }
    }

    fn actions(target: Target, s: Option<&Session>) -> Vec<Action> {
        entries(&target, s).into_iter().map(|(a, _)| a).collect()
    }

    #[test]
    fn a_session_adroit_runs_can_be_opened_renamed_started_beside_and_ended() {
        let s = session(Kind::Own);
        assert_eq!(
            actions(Target::Row(Item::Session("a".into())), Some(&s)),
            [
                Action::CursorOpen,
                Action::CursorShow,
                Action::SessionRename,
                Action::SessionNew,
                Action::SessionNewReader,
                Action::ShellOpen,
                Action::SessionStop,
            ]
        );
    }

    #[test]
    fn an_interactive_session_is_taken_over_rather_than_stopped() {
        let s = session(Kind::Interactive);
        let got = actions(Target::Row(Item::Waiting("a".into())), Some(&s));
        assert!(got.contains(&Action::SessionTakeOver));
        assert!(!got.contains(&Action::SessionStop));
        // Nor renamed, as it takes no message from here.
        assert!(!got.contains(&Action::SessionRename));
    }

    #[test]
    fn a_workspace_or_worktree_starts_sessions_and_shells_in_it() {
        let here = [
            Action::SessionNew,
            Action::SessionNewReader,
            Action::ShellOpen,
        ];
        assert_eq!(
            actions(Target::Row(Item::Workspace("/api".into())), None),
            [&[Action::CursorOpen][..], &here].concat()
        );
        assert_eq!(
            actions(Target::Row(Item::Worktree("/api".into())), None),
            [&[Action::CursorOpen, Action::CursorShow][..], &here].concat()
        );
    }

    #[test]
    fn a_shell_s_row_opens_it_starts_beside_it_and_closes_it() {
        assert_eq!(
            actions(Target::Row(Item::Shell(1)), None),
            [
                Action::CursorOpen,
                Action::CursorShow,
                Action::SessionNew,
                Action::SessionNewReader,
                Action::ShellOpen,
                Action::TabClose,
            ]
        );
    }

    #[test]
    fn a_tab_closes_and_a_session_s_tab_also_ends_it() {
        let s = session(Kind::Own);
        assert_eq!(
            actions(Target::Tab(Tab::Session("a".into())), Some(&s)),
            [Action::TabClose, Action::SessionRename, Action::SessionStop]
        );
        assert_eq!(
            actions(Target::Tab(Tab::Shell(1)), None),
            [Action::TabClose]
        );
    }
}
