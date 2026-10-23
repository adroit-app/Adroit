//! What a right click on a sidebar row, a tab or a link of the details
//! panel's overview offers: the actions its keys would run there.

use adroit_model::{Kind, Session};

use crate::keymap::Action;
use crate::nav::Item;
use crate::tabs::Tab;

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

/// The actions offered on `target`, each with what the menu calls it.
/// `session` is the session the row or tab is for, when it is for one.
pub fn entries(target: &Target, session: Option<&Session>) -> Vec<(Action, &'static str)> {
    let here = [
        (Action::SessionNew, "New session here"),
        (Action::ShellOpen, "Open a shell here"),
        (Action::NeovimOpen, "Open Neovim here"),
    ];
    let ending = session.map(|s| match s.kind {
        Kind::Interactive => (Action::SessionTakeOver, "Take over…"),
        Kind::Own => (Action::SessionStop, "End…"),
    });
    // A session in another terminal is named there.
    let rename = session
        .filter(|s| s.kind == Kind::Own)
        .map(|_| (Action::SessionRename, "Rename…"));
    // The key says where a row opens, as in fern.
    let opens = [
        (Action::CursorGo, "Open"),
        (Action::CursorSplit, "Open in a split below"),
        (Action::CursorVsplit, "Open in a split beside"),
        (Action::CursorTab, "Open in a new tab"),
        (Action::CursorShow, "Show"),
    ];
    let copy_path = (Action::CursorCopy, "Copy path");
    let copy_name = (Action::CursorCopy, "Copy name");
    let close = (Action::CursorClose, "Close its windows");
    match target {
        Target::Row(Item::Session(_) | Item::Waiting(_)) => opens
            .into_iter()
            .chain(rename)
            .chain([copy_name])
            .chain(here)
            .chain([close])
            .chain(ending)
            .collect(),
        Target::Row(Item::Terminal(_)) => opens
            .into_iter()
            .chain([copy_name])
            .chain(here)
            .chain([close, (Action::TerminalStop, "Stop terminal")])
            .collect(),
        Target::Row(Item::File { dir: false, .. }) => opens
            .into_iter()
            .chain([copy_path])
            .chain(here)
            .chain([close])
            .collect(),
        Target::Row(Item::Worktree(_)) => {
            opens.into_iter().chain([copy_path]).chain(here).collect()
        }
        Target::Row(Item::Workspace(_) | Item::File { dir: true, .. }) => {
            [(Action::CursorGo, "Fold or unfold"), copy_path]
                .into_iter()
                .chain(here)
                .collect()
        }
        Target::Row(Item::Heading(..)) => [(Action::CursorGo, "Fold or unfold")]
            .into_iter()
            .chain(here)
            .collect(),
        Target::Tab(Tab::Session(_)) => [(Action::TabClose, "Close tab")]
            .into_iter()
            .chain(rename)
            .chain(ending)
            .collect(),
        Target::Tab(Tab::Terminal(_)) => vec![
            (Action::TabClose, "Close tab"),
            (Action::TerminalStop, "Stop terminal"),
        ],
        Target::Tab(Tab::Draft { .. }) => Vec::new(),
        Target::Tab(Tab::Viewer(_)) => vec![(Action::TabClose, "Close tab")],
        Target::Link { file, .. } => [(Action::CursorOpen, "Open"), (Action::CursorCopy, "Copy")]
            .into_iter()
            .chain(file.then_some((Action::CursorReveal, "Reveal in Finder")))
            .collect(),
    }
}

/// [`entries`] in the groups the menu draws apart: going to what was
/// clicked, starting something beside it, and ending or closing it.
pub fn groups(target: &Target, session: Option<&Session>) -> Vec<Vec<(Action, &'static str)>> {
    let kin = |a: &Action| match a {
        Action::SessionNew | Action::ShellOpen | Action::NeovimOpen => 1,
        Action::SessionTakeOver
        | Action::SessionStop
        | Action::TerminalStop
        | Action::TabClose
        | Action::CursorClose => 2,
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
            Target::Row(Item::Terminal(1)),
            Target::Tab(Tab::Session("a".into())),
            Target::Tab(Tab::Terminal(1)),
            Target::Link { at: 0, file: true },
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
    fn a_link_of_the_overview_is_opened_or_copied_and_a_file_revealed_too() {
        assert_eq!(
            actions(Target::Link { at: 2, file: false }, None),
            [Action::CursorOpen, Action::CursorCopy]
        );
        assert_eq!(
            actions(Target::Link { at: 2, file: true }, None),
            [Action::CursorOpen, Action::CursorCopy, Action::CursorReveal]
        );
    }

    #[test]
    fn a_menu_draws_going_starting_and_ending_apart() {
        let s = session(Kind::Own);
        assert_eq!(
            grouped(Target::Row(Item::Session("a".into())), Some(&s)),
            [
                [&OPENS[..], &[Action::SessionRename, Action::CursorCopy]].concat(),
                vec![Action::SessionNew, Action::ShellOpen, Action::NeovimOpen],
                vec![Action::CursorClose, Action::SessionStop],
            ]
        );
        assert_eq!(
            grouped(Target::Tab(Tab::Terminal(1)), None),
            [vec![Action::TabClose, Action::TerminalStop]]
        );
        let draft = Tab::Draft {
            worktree: "/a".into(),
            id: Default::default(),
        };
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
        let here = [Action::SessionNew, Action::ShellOpen, Action::NeovimOpen];
        assert_eq!(
            actions(Target::Row(Item::Workspace("/api".into())), None),
            [&[Action::CursorGo, Action::CursorCopy][..], &here].concat()
        );
        assert_eq!(
            actions(Target::Row(Item::Worktree("/api".into())), None),
            [&OPENS[..], &[Action::CursorCopy], &here].concat()
        );
    }

    #[test]
    fn a_terminal_row_opens_it_starts_beside_it_and_offers_an_explicit_stop() {
        assert_eq!(
            actions(Target::Row(Item::Terminal(1)), None),
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
            Target::Row(Item::Heading("/api".into(), crate::nav::Heading::Files)),
            None,
        );
        assert_eq!(got[0], Action::CursorGo);
        assert!(!got.contains(&Action::CursorCopy));
    }

    #[test]
    fn closing_a_tab_and_stopping_its_agent_or_terminal_are_distinct_actions() {
        let s = session(Kind::Own);
        assert_eq!(
            actions(Target::Tab(Tab::Session("a".into())), Some(&s)),
            [Action::TabClose, Action::SessionRename, Action::SessionStop]
        );
        assert_eq!(
            actions(Target::Tab(Tab::Terminal(1)), None),
            [Action::TabClose, Action::TerminalStop]
        );
    }
}
