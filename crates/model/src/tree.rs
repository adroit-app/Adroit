use std::collections::HashMap;
use std::path::{Path, PathBuf};

use crate::{Session, Shell};

/// A git worktree as found on disk.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Worktree {
    pub path: PathBuf,
    /// The main checkout of the repository the worktree belongs to.
    pub repo: PathBuf,
    pub branch: Option<String>,
}

impl Worktree {
    /// A directory outside git, standing as its own only worktree.
    pub fn plain(dir: impl Into<PathBuf>) -> Self {
        let dir = dir.into();
        Worktree {
            path: dir.clone(),
            repo: dir,
            branch: None,
        }
    }

    /// Whether this is the repository's main checkout.
    pub fn is_main(&self) -> bool {
        self.path == self.repo
    }
}

/// The workspaces sessions and shells run in, each with its worktrees, and
/// what runs in each.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Tree {
    pub workspaces: Vec<WorkspaceNode>,
}

/// A git repository, or a directory outside git, and what runs in it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkspaceNode {
    pub root: PathBuf,
    pub name: String,
    pub git: bool,
    pub worktrees: Vec<WorktreeNode>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorktreeNode {
    pub worktree: Worktree,
    pub sessions: Vec<Session>,
    pub shells: Vec<Shell>,
}

impl WorkspaceNode {
    /// Whether the sessions sit straight under the workspace, with no
    /// worktree row: outside git, or when only the main checkout is listed.
    pub fn flat(&self) -> bool {
        match self.worktrees.as_slice() {
            [only] => !self.git || only.worktree.is_main(),
            _ => false,
        }
    }

    pub fn sessions(&self) -> impl Iterator<Item = &Session> {
        self.worktrees.iter().flat_map(|w| w.sessions.iter())
    }
}

impl Tree {
    /// Every session with the name of the workspace it runs in.
    pub fn sessions(&self) -> impl Iterator<Item = (&str, &Session)> {
        self.workspaces
            .iter()
            .flat_map(|w| w.sessions().map(move |s| (w.name.as_str(), s)))
    }

    /// Every worktree of every workspace.
    pub fn worktrees(&self) -> impl Iterator<Item = &WorktreeNode> {
        self.workspaces.iter().flat_map(|ws| ws.worktrees.iter())
    }

    /// The workspace and worktree a session runs in.
    pub fn find(&self, id: &str) -> Option<(&WorkspaceNode, &WorktreeNode, &Session)> {
        self.workspaces.iter().find_map(|ws| {
            ws.worktrees
                .iter()
                .find_map(|wt| wt.sessions.iter().find(|s| s.id == id).map(|s| (ws, wt, s)))
        })
    }

    /// The workspace and worktree a shell runs in.
    pub fn shell(&self, id: u128) -> Option<(&WorkspaceNode, &WorktreeNode, &Shell)> {
        self.workspaces.iter().find_map(|ws| {
            ws.worktrees
                .iter()
                .find_map(|wt| wt.shells.iter().find(|s| s.id == id).map(|s| (ws, wt, s)))
        })
    }

    /// The tree with `shells` in place of the shells it held, each in the
    /// innermost worktree holding the directory it runs in. A shell no
    /// worktree holds is left out: placing it needs git, which `build` asks.
    pub fn with_shells(&self, shells: &[Shell]) -> Tree {
        let mut out = self.clone();
        for w in out
            .workspaces
            .iter_mut()
            .flat_map(|ws| ws.worktrees.iter_mut())
        {
            w.shells.clear();
        }
        let paths: Vec<PathBuf> = out.worktrees().map(|w| w.worktree.path.clone()).collect();
        for shell in shells {
            let Some(home) = innermost(&paths, &shell.worktree).cloned() else {
                continue;
            };
            if let Some(w) = out
                .workspaces
                .iter_mut()
                .flat_map(|ws| ws.worktrees.iter_mut())
                .find(|w| w.worktree.path == home)
            {
                w.shells.push(shell.clone());
            }
        }
        out
    }

    /// The workspace holding `path`, a worktree of it or the directory itself.
    pub fn workspace_of(&self, path: &Path) -> Option<&WorkspaceNode> {
        self.workspaces
            .iter()
            .find(|ws| ws.root == path || ws.worktrees.iter().any(|w| w.worktree.path == path))
    }
}

/// Build the tree. `found` tells, for each directory a session or a shell
/// runs in, the git worktree holding it, or `None` outside git; a directory
/// missing from it is taken as outside git. `listed` adds worktrees without
/// sessions, those git knows of a workspace when every worktree is to be shown.
pub fn build(
    sessions: &[Session],
    shells: &[Shell],
    found: &HashMap<PathBuf, Option<Worktree>>,
    listed: &[Worktree],
) -> Tree {
    let mut tree = Tree::default();
    for session in sessions {
        let (worktree, git) = match found.get(&session.cwd).cloned().flatten() {
            Some(w) => (w, true),
            None => (Worktree::plain(&session.cwd), false),
        };
        worktree_node(&mut tree, &worktree, git)
            .sessions
            .push(session.clone());
    }
    for shell in shells {
        match found.get(&shell.worktree).cloned().flatten() {
            Some(w) => worktree_node(&mut tree, &w, true),
            None => worktree_node(&mut tree, &Worktree::plain(&shell.worktree), false),
        };
    }
    for w in listed {
        if tree.workspaces.iter().any(|ws| ws.git && ws.root == w.repo) {
            worktree_node(&mut tree, w, true);
        }
    }
    tree.workspaces
        .sort_by(|a, b| (a.name.to_lowercase(), &a.root).cmp(&(b.name.to_lowercase(), &b.root)));
    for ws in &mut tree.workspaces {
        ws.worktrees.sort_by(|a, b| {
            (!a.worktree.is_main(), &a.worktree.path)
                .cmp(&(!b.worktree.is_main(), &b.worktree.path))
        });
    }
    tree.with_shells(shells)
}

/// The innermost of `worktrees` holding `cwd`. A worktree under
/// `.claude/worktrees/` sits inside the main checkout, so the nearest one wins.
pub fn innermost<'a, T: AsRef<Path> + 'a>(
    worktrees: impl IntoIterator<Item = &'a T>,
    cwd: &Path,
) -> Option<&'a T> {
    worktrees
        .into_iter()
        .filter(|w| cwd.starts_with(w))
        .max_by_key(|w| w.as_ref().components().count())
}

fn worktree_node<'a>(tree: &'a mut Tree, w: &Worktree, git: bool) -> &'a mut WorktreeNode {
    let wi = match tree.workspaces.iter().position(|ws| ws.root == w.repo) {
        Some(i) => i,
        None => {
            tree.workspaces.push(WorkspaceNode {
                root: w.repo.clone(),
                name: w
                    .repo
                    .file_name()
                    .map(|n| n.to_string_lossy().into_owned())
                    .unwrap_or_else(|| w.repo.display().to_string()),
                git,
                worktrees: Vec::new(),
            });
            tree.workspaces.len() - 1
        }
    };
    let ws = &mut tree.workspaces[wi];
    let ti = match ws.worktrees.iter().position(|n| n.worktree.path == w.path) {
        Some(i) => i,
        None => {
            ws.worktrees.push(WorktreeNode {
                worktree: w.clone(),
                sessions: Vec::new(),
                shells: Vec::new(),
            });
            ws.worktrees.len() - 1
        }
    };
    &mut ws.worktrees[ti]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Kind;
    use crate::Shell;

    fn session(id: &str, cwd: &str) -> Session {
        Session {
            id: id.into(),
            name: id.into(),
            cwd: cwd.into(),
            kind: Kind::Own,
            ..Default::default()
        }
    }

    fn wt(path: &str, repo: &str, branch: &str) -> Worktree {
        Worktree {
            path: path.into(),
            repo: repo.into(),
            branch: Some(branch.into()),
        }
    }

    fn found(entries: &[(&str, Option<Worktree>)]) -> HashMap<PathBuf, Option<Worktree>> {
        entries
            .iter()
            .map(|(cwd, w)| (PathBuf::from(cwd), w.clone()))
            .collect()
    }

    fn ids(node: &WorktreeNode) -> Vec<&str> {
        node.sessions.iter().map(|s| s.id.as_str()).collect()
    }

    #[test]
    fn sessions_in_worktrees_of_one_repository_share_its_workspace() {
        let t = build(
            &[
                session("a", "/api"),
                session("b", "/api/.claude/worktrees/x"),
            ],
            &[],
            &found(&[
                ("/api", Some(wt("/api", "/api", "main"))),
                (
                    "/api/.claude/worktrees/x",
                    Some(wt("/api/.claude/worktrees/x", "/api", "x")),
                ),
            ]),
            &[],
        );
        assert_eq!(t.workspaces.len(), 1);
        let ws = &t.workspaces[0];
        assert_eq!(ws.name, "api");
        assert_eq!(ids(&ws.worktrees[0]), vec!["a"]);
        assert_eq!(ids(&ws.worktrees[1]), vec!["b"]);
        assert!(!ws.flat());
        assert_eq!(t.worktrees().count(), 2);
    }

    #[test]
    fn a_session_below_a_worktree_s_top_sits_in_that_worktree() {
        let t = build(
            &[session("a", "/api/src")],
            &[],
            &found(&[("/api/src", Some(wt("/api", "/api", "main")))]),
            &[],
        );
        assert_eq!(
            t.workspaces[0].worktrees[0].worktree.path,
            PathBuf::from("/api")
        );
    }

    #[test]
    fn a_directory_outside_git_is_a_workspace_of_its_own() {
        let t = build(
            &[session("a", "/notes"), session("b", "/notes/sub")],
            &[],
            &found(&[("/notes", None)]),
            &[],
        );
        assert_eq!(t.workspaces.len(), 2);
        assert!(t.workspaces.iter().all(|ws| !ws.git && ws.flat()));
    }

    #[test]
    fn a_workspace_with_sessions_only_in_its_main_checkout_is_flat() {
        let t = build(
            &[session("a", "/api")],
            &[],
            &found(&[("/api", Some(wt("/api", "/api", "main")))]),
            &[],
        );
        assert!(t.workspaces[0].flat());
    }

    #[test]
    fn sessions_only_in_another_worktree_keep_the_worktree_row() {
        let t = build(
            &[session("a", "/api/.claude/worktrees/x")],
            &[],
            &found(&[(
                "/api/.claude/worktrees/x",
                Some(wt("/api/.claude/worktrees/x", "/api", "x")),
            )]),
            &[],
        );
        assert!(!t.workspaces[0].flat());
    }

    #[test]
    fn listed_worktrees_are_added_empty_to_workspaces_already_shown() {
        let t = build(
            &[session("a", "/api")],
            &[],
            &found(&[("/api", Some(wt("/api", "/api", "main")))]),
            &[
                wt("/api", "/api", "main"),
                wt("/api/.claude/worktrees/y", "/api", "y"),
                wt("/web", "/web", "main"),
            ],
        );
        assert_eq!(t.workspaces.len(), 1);
        let ws = &t.workspaces[0];
        assert_eq!(ws.worktrees.len(), 2);
        assert!(ws.worktrees[1].sessions.is_empty());
        assert!(!ws.flat());
    }

    #[test]
    fn workspaces_go_by_name_and_the_main_checkout_comes_first() {
        let t = build(
            &[
                session("w", "/src/web/.claude/worktrees/b"),
                session("m", "/src/web"),
                session("a", "/src/Api"),
            ],
            &[],
            &found(&[
                ("/src/web", Some(wt("/src/web", "/src/web", "main"))),
                (
                    "/src/web/.claude/worktrees/b",
                    Some(wt("/src/web/.claude/worktrees/b", "/src/web", "b")),
                ),
                ("/src/Api", None),
            ]),
            &[],
        );
        let names: Vec<_> = t.workspaces.iter().map(|w| w.name.as_str()).collect();
        assert_eq!(names, vec!["Api", "web"]);
        assert_eq!(ids(&t.workspaces[1].worktrees[0]), vec!["m"]);
    }

    #[test]
    fn a_session_is_found_with_its_workspace_and_worktree() {
        let t = build(
            &[session("a", "/api")],
            &[],
            &found(&[("/api", Some(wt("/api", "/api", "main")))]),
            &[],
        );
        let (ws, wt, s) = t.find("a").unwrap();
        assert_eq!(
            (ws.name.as_str(), wt.worktree.path.as_path(), s.id.as_str()),
            ("api", Path::new("/api"), "a")
        );
        assert!(t.find("b").is_none());
    }

    fn shell(id: u128, dir: &str) -> Shell {
        Shell {
            id,
            name: format!("shell-{id}"),
            worktree: dir.into(),
        }
    }

    fn shell_ids(node: &WorktreeNode) -> Vec<u128> {
        node.shells.iter().map(|s| s.id).collect()
    }

    #[test]
    fn a_worktree_where_only_shells_run_is_listed_with_them() {
        let x = "/api/.claude/worktrees/x";
        let t = build(
            &[session("a", "/api")],
            &[shell(1, x), shell(2, x)],
            &found(&[
                ("/api", Some(wt("/api", "/api", "main"))),
                (x, Some(wt(x, "/api", "x"))),
            ]),
            &[],
        );
        let ws = &t.workspaces[0];
        assert_eq!(ws.worktrees.len(), 2);
        assert!(ws.worktrees[1].sessions.is_empty());
        assert_eq!(shell_ids(&ws.worktrees[1]), [1, 2]);
        assert!(!ws.flat());
    }

    #[test]
    fn a_shell_below_a_worktree_s_top_sits_in_that_worktree_beside_its_sessions() {
        let t = build(
            &[session("a", "/api")],
            &[shell(1, "/api/src")],
            &found(&[
                ("/api", Some(wt("/api", "/api", "main"))),
                ("/api/src", Some(wt("/api", "/api", "main"))),
            ]),
            &[],
        );
        let ws = &t.workspaces[0];
        assert_eq!(ws.worktrees.len(), 1);
        assert_eq!(ids(&ws.worktrees[0]), ["a"]);
        assert_eq!(shell_ids(&ws.worktrees[0]), [1]);
        assert!(ws.flat());
    }

    #[test]
    fn a_shell_outside_git_makes_its_directory_a_workspace() {
        let t = build(&[], &[shell(1, "/notes")], &found(&[("/notes", None)]), &[]);
        let ws = &t.workspaces[0];
        assert_eq!(
            (ws.name.as_str(), ws.git, ws.flat()),
            ("notes", false, true)
        );
        assert_eq!(shell_ids(&ws.worktrees[0]), [1]);
    }

    #[test]
    fn a_shell_s_workspace_takes_the_worktrees_listed_for_it() {
        let t = build(
            &[],
            &[shell(1, "/api")],
            &found(&[("/api", Some(wt("/api", "/api", "main")))]),
            &[wt("/api", "/api", "main"), wt("/api/w", "/api", "w")],
        );
        assert_eq!(t.worktrees().count(), 2);
    }

    #[test]
    fn shells_placed_again_go_to_the_innermost_worktree_holding_them() {
        let x = "/api/.claude/worktrees/x";
        let t = build(
            &[session("a", "/api"), session("b", x)],
            &[shell(1, "/api")],
            &found(&[
                ("/api", Some(wt("/api", "/api", "main"))),
                (x, Some(wt(x, "/api", "x"))),
            ]),
            &[],
        );
        // One opened since in the nested worktree, one in a directory no
        // worktree listed holds yet; the first is gone.
        let again = t.with_shells(&[shell(2, &format!("{x}/src")), shell(3, "/web")]);
        let ws = &again.workspaces[0];
        assert!(ws.worktrees[0].shells.is_empty());
        assert_eq!(shell_ids(&ws.worktrees[1]), [2]);
        assert_eq!(again.workspaces.len(), 1);
    }

    #[test]
    fn a_shell_is_found_with_its_workspace_and_worktree() {
        let t = build(
            &[],
            &[shell(1, "/api")],
            &found(&[("/api", Some(wt("/api", "/api", "main")))]),
            &[],
        );
        let (ws, wt, s) = t.shell(1).unwrap();
        assert_eq!(
            (ws.name.as_str(), wt.worktree.path.as_path(), s.id),
            ("api", Path::new("/api"), 1)
        );
        assert!(t.shell(2).is_none());
    }

    #[test]
    fn iterating_sessions_names_each_ones_workspace() {
        let t = build(
            &[session("a", "/api"), session("b", "/notes")],
            &[],
            &found(&[("/api", Some(wt("/api", "/api", "main")))]),
            &[],
        );
        let got: Vec<_> = t.sessions().map(|(w, s)| (w, s.id.as_str())).collect();
        assert_eq!(got, vec![("api", "a"), ("notes", "b")]);
    }
}
