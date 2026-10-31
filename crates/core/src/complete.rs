//! What `@` in the prompt offers, and where `/` finds the commands it
//! offers.

use std::path::PathBuf;

use adroit_model::Tree;

use crate::content::Tab;
use crate::tree::nav::{self, Place};

/// What `@` offers: a word, and what it is.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Offer {
    pub word: String,
    pub kind: &'static str,
}

impl Offer {
    pub fn session(name: impl Into<String>) -> Self {
        Self {
            word: name.into(),
            kind: "session",
        }
    }

    pub fn file(path: impl Into<String>) -> Self {
        Self {
            word: path.into(),
            kind: "file",
        }
    }
}

/// The sessions to talk to, those of the open workspace first, then the files
/// of the open worktree.
pub fn candidates(tree: &Tree, place: Option<&Place>, files: &[String]) -> Vec<Offer> {
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
        .map(|s| Offer::session(s.name.clone()))
        .chain(files.iter().cloned().map(Offer::file))
        .collect()
}

/// The directory whose commands `/` offers while `tab` is shown, or the
/// open worktree with no tab: where its session runs, or would start. A shell
/// or a viewer takes no commands.
pub fn commands_dir(tree: &Tree, tab: Option<&Tab>, place: Option<&Place>) -> Option<PathBuf> {
    match tab {
        Some(Tab::Session(_, id)) => nav::find(tree, id).map(|(_, s)| s.cwd.clone()),
        Some(Tab::Draft { worktree, .. }) => Some(worktree.clone()),
        Some(Tab::Terminal(_, _) | Tab::Viewer(_, _) | Tab::Drawn { .. }) => None,
        None => place.map(|p| p.worktree.clone()),
    }
}

#[cfg(test)]
mod tests {
    use crate::testing::here;
    use adroit_model::testing::{self, session};

    use super::*;

    fn tree_with(name: &str, cwd: &str) -> Tree {
        testing::tree().session(session(name).cwd(cwd)).build()
    }

    #[test]
    fn the_open_workspace_s_sessions_come_before_the_others_and_files_last() {
        let x = "/api/.claude/worktrees/x";
        let tree = testing::tree()
            .session(session("scratch").cwd("/tmp"))
            .session(session("fix").cwd(x))
            .checkout(x, "/api", "x")
            .build();
        let place = Place::at(&here(), "/api");
        assert_eq!(
            candidates(&tree, Some(&place), &["src/lib.rs".into()]),
            vec![
                Offer::session("fix"),
                Offer::session("scratch"),
                Offer::file("src/lib.rs"),
            ]
        );
    }

    #[test]
    fn a_workspace_of_the_same_name_elsewhere_is_not_the_open_one() {
        let tree = testing::tree()
            .session(session("there").cwd("/a/api"))
            .session(session("here").cwd("/b/api"))
            .build();
        let place = Place::at(&here(), "/b/api");
        let got: Vec<_> = candidates(&tree, Some(&place), &[])
            .into_iter()
            .map(|c| c.word)
            .collect();
        assert_eq!(got, vec!["here".to_string(), "there".to_string()]);
    }

    #[test]
    fn a_session_s_commands_are_those_of_where_it_runs() {
        let tree = tree_with("fix", "/api/.claude/worktrees/x");
        let tab = Tab::Session(here(), "fix".into());
        assert_eq!(
            commands_dir(&tree, Some(&tab), None),
            Some(PathBuf::from("/api/.claude/worktrees/x"))
        );
    }

    #[test]
    fn a_draft_s_commands_are_those_of_its_worktree() {
        let tab = Tab::draft(here(), "/api");
        assert_eq!(
            commands_dir(&Tree::default(), Some(&tab), None),
            Some(PathBuf::from("/api"))
        );
    }

    #[test]
    fn with_nothing_shown_the_commands_are_those_of_the_open_worktree() {
        let place = Place::at(&here(), "/api");
        assert_eq!(
            commands_dir(&Tree::default(), None, Some(&place)),
            Some(PathBuf::from("/api"))
        );
    }

    #[test]
    fn a_shell_takes_no_commands() {
        let place = Place::at(&here(), "/api");
        assert_eq!(
            commands_dir(
                &Tree::default(),
                Some(&Tab::Terminal(here(), 1)),
                Some(&place)
            ),
            None
        );
    }
}
