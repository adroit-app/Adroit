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
        Target::Row(Item::Session(_) | Item::Waiting(_)) => [(Action::CursorOpen, "Open")]
            .into_iter()
            .chain(rename)
            .chain(here)
            .chain(ending)
            .collect(),
        Target::Row(Item::Worktree(_)) => [(Action::CursorOpen, "Open")]
            .into_iter()
            .chain(here)
            .collect(),
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

#[cfg(test)]
mod tests {
    use super::*;

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
        for row in [
            Item::Workspace("/api".into()),
            Item::Worktree("/api".into()),
        ] {
            assert_eq!(
                actions(Target::Row(row), None),
                [
                    Action::CursorOpen,
                    Action::SessionNew,
                    Action::SessionNewReader,
                    Action::ShellOpen,
                ]
            );
        }
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
