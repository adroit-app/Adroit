//! What `@` and `/` in the prompt offer.

use std::path::PathBuf;

use adroit_claude::commands::Command;
use adroit_model::Tree;
use adroit_nvim::Candidate;

use crate::nav::{self, Place};
use crate::tabs::Tab;

/// The sessions to talk to, those of the open workspace first, then the files
/// of the open worktree.
pub fn candidates(tree: &Tree, place: Option<&Place>, files: &[String]) -> Vec<Candidate> {
    // Told by its root: two repositories of one name are two workspaces.
    let here = place.and_then(|p| tree.workspace_of(&p.worktree));
    let far = tree
        .workspaces
        .iter()
        .filter(|ws| here.is_none_or(|h| h.root != ws.root))
        .flat_map(|ws| ws.sessions());
    here.into_iter()
        .flat_map(|ws| ws.sessions())
        .chain(far)
        .map(|s| Candidate::new(s.name.clone(), "session"))
        .chain(files.iter().map(|f| Candidate::new(f.clone(), "file")))
        .collect()
}

/// The directory whose commands `/` offers while `tab` is shown, or the
/// open worktree with no tab: where its session runs, or would start. A shell
/// takes no commands.
pub fn commands_dir(tree: &Tree, tab: Option<&Tab>, place: Option<&Place>) -> Option<PathBuf> {
    match tab {
        Some(Tab::Session(id)) => nav::find(tree, id).map(|(_, s)| s.cwd.clone()),
        Some(Tab::Draft { worktree, .. }) => Some(worktree.clone()),
        Some(Tab::Shell(_)) => None,
        None => place.map(|p| p.worktree.clone()),
    }
}

/// The most characters of a command's description shown beside it.
const DESCRIPTION_SHOWN: usize = 60;

/// What `/` offers of Claude Code's commands: each by name, what it does
/// shortened beside it, and while picked, its arguments and what it does whole.
pub fn commands(list: &[Command]) -> Vec<Candidate> {
    list.iter()
        .map(|c| {
            let word = format!("/{}", c.name);
            let long = c.description.chars().count() > DESCRIPTION_SHOWN;
            let menu = if long {
                let kept: String = c.description.chars().take(DESCRIPTION_SHOWN - 1).collect();
                format!("{}…", kept.trim_end())
            } else {
                c.description.clone()
            };
            let usage =
                (!c.argument_hint.is_empty()).then(|| format!("{word} {}", c.argument_hint));
            let info = match (usage, long) {
                (Some(usage), true) => format!("{usage} — {}", c.description),
                (Some(usage), false) => usage,
                (None, true) => c.description.clone(),
                (None, false) => String::new(),
            };
            Candidate::new(word, menu).with_info(info)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;
    use std::path::PathBuf;

    use adroit_model::{Kind, Session, State, Worktree, build};

    use super::*;

    #[test]
    fn the_open_workspace_s_sessions_come_before_the_others_and_files_last() {
        let s = |name: &str, cwd: &str| Session {
            id: name.into(),
            name: name.into(),
            cwd: cwd.into(),
            kind: Kind::Own,
            state: State::Idle,
            ..Default::default()
        };
        let found = HashMap::from([(
            PathBuf::from("/api/.claude/worktrees/x"),
            Some(Worktree {
                path: "/api/.claude/worktrees/x".into(),
                repo: "/api".into(),
                branch: None,
            }),
        )]);
        let tree = build(
            &[s("scratch", "/tmp"), s("fix", "/api/.claude/worktrees/x")],
            &[],
            &found,
            &[],
            None,
        );
        let place = Place {
            worktree: "/api".into(),
        };
        let got: Vec<_> = candidates(&tree, Some(&place), &["src/lib.rs".into()])
            .into_iter()
            .map(|c| (c.word, c.menu))
            .collect();
        assert_eq!(
            got,
            vec![
                ("fix".into(), "session".into()),
                ("scratch".into(), "session".into()),
                ("src/lib.rs".into(), "file".into()),
            ]
        );
    }

    #[test]
    fn a_workspace_of_the_same_name_elsewhere_is_not_the_open_one() {
        let s = |name: &str, cwd: &str| Session {
            id: name.into(),
            name: name.into(),
            cwd: cwd.into(),
            kind: Kind::Own,
            ..Default::default()
        };
        let tree = build(
            &[s("there", "/a/api"), s("here", "/b/api")],
            &[],
            &HashMap::new(),
            &[],
            None,
        );
        let place = Place {
            worktree: "/b/api".into(),
        };
        let got: Vec<_> = candidates(&tree, Some(&place), &[])
            .into_iter()
            .map(|c| c.word)
            .collect();
        assert_eq!(got, vec!["here".to_string(), "there".to_string()]);
    }
    fn command(name: &str, description: &str, argument_hint: &str) -> Command {
        Command {
            name: name.into(),
            description: description.into(),
            argument_hint: argument_hint.into(),
        }
    }

    #[test]
    fn a_command_is_offered_by_its_name_with_what_it_does_beside_it() {
        let got = commands(&[command("compact", "Clear history but keep a summary", "")]);
        assert_eq!(
            got,
            [Candidate::new(
                "/compact",
                "Clear history but keep a summary"
            )]
        );
    }

    #[test]
    fn a_long_description_is_shortened_beside_and_whole_while_picked() {
        let long = "あ".repeat(80);
        let got = commands(&[command("ja", &long, "")]);
        assert_eq!(got[0].menu.chars().count(), DESCRIPTION_SHOWN);
        assert!(got[0].menu.ends_with('…'));
        assert_eq!(got[0].info, long);
    }

    #[test]
    fn the_arguments_a_command_takes_are_shown_while_it_is_picked() {
        let got = commands(&[command("review", "Review a pull request", "[pr]")]);
        assert_eq!(got[0].info, "/review [pr]");
        let got = commands(&[command("ja", &"x".repeat(80), "[pr]")]);
        assert_eq!(got[0].info, format!("/ja [pr] — {}", "x".repeat(80)));
    }

    fn tree_with(name: &str, cwd: &str) -> Tree {
        let s = Session {
            id: name.into(),
            name: name.into(),
            cwd: cwd.into(),
            kind: Kind::Own,
            ..Default::default()
        };
        build(&[s], &[], &HashMap::new(), &[], None)
    }

    #[test]
    fn a_session_s_commands_are_those_of_where_it_runs() {
        let tree = tree_with("fix", "/api/.claude/worktrees/x");
        let tab = Tab::Session("fix".into());
        assert_eq!(
            commands_dir(&tree, Some(&tab), None),
            Some(PathBuf::from("/api/.claude/worktrees/x"))
        );
    }

    #[test]
    fn a_draft_s_commands_are_those_of_its_worktree() {
        let tab = Tab::Draft {
            worktree: "/api".into(),
            role: crate::tabs::Role::Worker,
        };
        assert_eq!(
            commands_dir(&Tree::default(), Some(&tab), None),
            Some(PathBuf::from("/api"))
        );
    }

    #[test]
    fn with_nothing_shown_the_commands_are_those_of_the_open_worktree() {
        let place = Place {
            worktree: "/api".into(),
        };
        assert_eq!(
            commands_dir(&Tree::default(), None, Some(&place)),
            Some(PathBuf::from("/api"))
        );
    }

    #[test]
    fn a_shell_takes_no_commands() {
        let place = Place {
            worktree: "/api".into(),
        };
        assert_eq!(
            commands_dir(&Tree::default(), Some(&Tab::Shell(1)), Some(&place)),
            None
        );
    }
}
