//! What `@` and `/` in the prompt offer.

use std::path::PathBuf;

use adroit_agents::claude::commands::Command;
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
/// or a viewer takes no commands.
pub fn commands_dir(tree: &Tree, tab: Option<&Tab>, place: Option<&Place>) -> Option<PathBuf> {
    match tab {
        Some(Tab::Session(id)) => nav::find(tree, id).map(|(_, s)| s.cwd.clone()),
        Some(Tab::Draft { worktree, .. }) => Some(worktree.clone()),
        Some(Tab::Terminal(_) | Tab::Viewer(_)) => None,
        None => place.map(|p| p.worktree.clone()),
    }
}

/// The most characters of a command's description shown beside it.
const DESCRIPTION_SHOWN: usize = 60;

/// What `/` offers of an agent's commands: each by name, what it does
/// shortened beside it, and while picked, its arguments and what it does whole.
pub fn commands(list: &[Command]) -> Vec<Candidate> {
    list.iter()
        .map(|c| {
            let word = if c.name.starts_with('$') {
                c.name.clone()
            } else {
                format!("/{}", c.name)
            };
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
    use adroit_model::testing::{self, session};

    use super::*;

    #[test]
    fn codex_skills_keep_the_dollar_invocation_in_the_completion() {
        let got = commands(&[command("$deploy", "Deploy the service", "")]);
        assert_eq!(got[0].word, "$deploy");
    }

    #[test]
    fn the_open_workspace_s_sessions_come_before_the_others_and_files_last() {
        let x = "/api/.claude/worktrees/x";
        let tree = testing::tree()
            .session(session("scratch").cwd("/tmp"))
            .session(session("fix").cwd(x))
            .checkout(x, "/api", "x")
            .build();
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
        let tree = testing::tree()
            .session(session("there").cwd("/a/api"))
            .session(session("here").cwd("/b/api"))
            .build();
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
        testing::tree().session(session(name).cwd(cwd)).build()
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
            id: Default::default(),
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
            commands_dir(&Tree::default(), Some(&Tab::Terminal(1)), Some(&place)),
            None
        );
    }

    #[tokio::test]
    async fn codex_slash_commands_reach_the_embedded_prompt() {
        let list = adroit_agents::adapter(adroit_model::Agent::Codex).builtin_commands();
        let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
        let editor = adroit_nvim::Embedded::spawn(
            "nvim",
            &["-u", "NONE", "-i", "NONE", "--noplugin"],
            80,
            10,
            move |event| {
                let _ = tx.send(event);
            },
        )
        .await
        .unwrap();
        editor.set_commands(&commands(&list)).await.unwrap();
        editor.input("/mo").await.unwrap();
        tokio::time::timeout(std::time::Duration::from_secs(5), async {
            loop {
                if editor
                    .state()
                    .popup()
                    .is_some_and(|p| p.items.iter().any(|c| c.word == "/model"))
                {
                    break;
                }
                rx.recv().await.expect("editor event");
            }
        })
        .await
        .expect("/model completion");
        editor.input("<C-y>").await.unwrap();
        assert_eq!(editor.take_text().await.unwrap(), "/model ");
    }
}
