//! What `@` in the prompt offers.

use adroit_model::Tree;
use adroit_nvim::Candidate;

use crate::nav::Place;

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
            &found,
            &[],
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
            &HashMap::new(),
            &[],
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
}
