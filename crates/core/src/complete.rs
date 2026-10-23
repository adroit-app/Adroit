//! Where `/` in the prompt finds the commands it offers.

use std::path::PathBuf;

use adroit_model::Tree;

use crate::content::Tab;
use crate::tree::nav::{self, Place};

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

#[cfg(test)]
mod tests {
    use adroit_model::testing::{self, session};

    use super::*;

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
}
