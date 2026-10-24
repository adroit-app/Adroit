use std::collections::HashMap;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::{Session, TerminalSession};

/// A git worktree as found on disk.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
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

/// The workspaces sessions and terminals run in, each with its worktrees, and
/// what runs in each.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Tree {
    pub workspaces: Vec<WorkspaceNode>,
}

/// A git repository, or a directory outside git, and what runs in it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorkspaceNode {
    pub root: PathBuf,
    pub name: String,
    pub git: bool,
    /// Whether this is the scratch root, holding every directory under it.
    pub scratch: bool,
    pub worktrees: Vec<WorktreeNode>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorktreeNode {
    pub worktree: Worktree,
    pub sessions: Vec<Session>,
    pub terminals: Vec<TerminalSession>,
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
    pub fn terminal(&self, id: u128) -> Option<(&WorkspaceNode, &WorktreeNode, &TerminalSession)> {
        self.workspaces.iter().find_map(|ws| {
            ws.worktrees.iter().find_map(|wt| {
                wt.terminals
                    .iter()
                    .find(|s| s.id == id)
                    .map(|s| (ws, wt, s))
            })
        })
    }

    /// The tree with `terminals` in place of the terminals it held, each in the
    /// innermost worktree holding the directory it runs in. A shell no
    /// worktree holds is left out: placing it needs git, which `build` asks.
    pub fn with_terminals(&self, terminals: &[TerminalSession]) -> Tree {
        let mut out = self.clone();
        for w in out
            .workspaces
            .iter_mut()
            .flat_map(|ws| ws.worktrees.iter_mut())
        {
            w.terminals.clear();
        }
        let paths: Vec<PathBuf> = out.worktrees().map(|w| w.worktree.path.clone()).collect();
        for shell in terminals {
            let Some(home) = innermost(&paths, &shell.cwd).cloned() else {
                continue;
            };
            if let Some(w) = out
                .workspaces
                .iter_mut()
                .flat_map(|ws| ws.worktrees.iter_mut())
                .find(|w| w.worktree.path == home)
            {
                w.terminals.push(shell.clone());
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
/// Whatever runs in `scratch` or below it, whatever git says, is in one
/// workspace of that directory, listed after every other.
pub fn build(
    sessions: &[Session],
    terminals: &[TerminalSession],
    found: &HashMap<PathBuf, Option<Worktree>>,
    listed: &[Worktree],
    scratch: Option<&Path>,
) -> Tree {
    let mut tree = Tree::default();
    let held = |dir: &Path| match scratch.filter(|root| dir.starts_with(root)) {
        Some(root) => (Worktree::plain(root), Holder::Scratch),
        None => match found.get(dir).cloned().flatten() {
            Some(w) => (w, Holder::Git),
            None => (Worktree::plain(dir), Holder::Plain),
        },
    };
    for session in sessions {
        let (worktree, holder) = held(&session.cwd);
        worktree_node(&mut tree, &worktree, holder)
            .sessions
            .push(session.clone());
    }
    for shell in terminals {
        let (worktree, holder) = held(&shell.cwd);
        worktree_node(&mut tree, &worktree, holder);
    }
    for w in listed {
        if tree.workspaces.iter().any(|ws| ws.git && ws.root == w.repo) {
            worktree_node(&mut tree, w, Holder::Git);
        }
    }
    tree.workspaces.sort_by(|a, b| {
        (a.scratch, a.name.to_lowercase(), &a.root).cmp(&(
            b.scratch,
            b.name.to_lowercase(),
            &b.root,
        ))
    });
    for ws in &mut tree.workspaces {
        ws.worktrees.sort_by(|a, b| {
            (!a.worktree.is_main(), &a.worktree.path)
                .cmp(&(!b.worktree.is_main(), &b.worktree.path))
        });
    }
    tree.with_terminals(terminals)
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

/// What a workspace is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Holder {
    Git,
    Plain,
    Scratch,
}

fn worktree_node<'a>(tree: &'a mut Tree, w: &Worktree, holder: Holder) -> &'a mut WorktreeNode {
    let wi = match tree.workspaces.iter().position(|ws| ws.root == w.repo) {
        Some(i) => i,
        None => {
            tree.workspaces.push(WorkspaceNode {
                root: w.repo.clone(),
                name: match holder {
                    Holder::Scratch => "Scratch".into(),
                    _ => w
                        .repo
                        .file_name()
                        .map(|n| n.to_string_lossy().into_owned())
                        .unwrap_or_else(|| w.repo.display().to_string()),
                },
                git: holder == Holder::Git,
                scratch: holder == Holder::Scratch,
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
                terminals: Vec::new(),
            });
            ws.worktrees.len() - 1
        }
    };
    &mut ws.worktrees[ti]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::{session, shell, tree, worktree};

    fn ids(node: &WorktreeNode) -> Vec<&str> {
        node.sessions.iter().map(|s| s.id.as_str()).collect()
    }

    #[test]
    fn sessions_in_worktrees_of_one_repository_share_its_workspace() {
        let x = "/api/.claude/worktrees/x";
        let t = tree()
            .session(session("a").cwd("/api"))
            .session(session("b").cwd(x))
            .checkout("/api", "/api", "main")
            .checkout(x, "/api", "x")
            .build();
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
        let t = tree()
            .session(session("a").cwd("/api/src"))
            .found("/api/src", Some(worktree("/api", "/api", "main")))
            .build();
        assert_eq!(
            t.workspaces[0].worktrees[0].worktree.path,
            PathBuf::from("/api")
        );
    }

    #[test]
    fn a_directory_outside_git_is_a_workspace_of_its_own() {
        let t = tree()
            .session(session("a").cwd("/notes"))
            .session(session("b").cwd("/notes/sub"))
            .found("/notes", None)
            .build();
        assert_eq!(t.workspaces.len(), 2);
        assert!(t.workspaces.iter().all(|ws| !ws.git && ws.flat()));
    }

    #[test]
    fn a_workspace_with_sessions_only_in_its_main_checkout_is_flat() {
        let t = tree()
            .session(session("a").cwd("/api"))
            .checkout("/api", "/api", "main")
            .build();
        assert!(t.workspaces[0].flat());
    }

    #[test]
    fn sessions_only_in_another_worktree_keep_the_worktree_row() {
        let x = "/api/.claude/worktrees/x";
        let t = tree()
            .session(session("a").cwd(x))
            .checkout(x, "/api", "x")
            .build();
        assert!(!t.workspaces[0].flat());
    }

    #[test]
    fn listed_worktrees_are_added_empty_to_workspaces_already_shown() {
        let t = tree()
            .session(session("a").cwd("/api"))
            .checkout("/api", "/api", "main")
            .listed(worktree("/api", "/api", "main"))
            .listed(worktree("/api/.claude/worktrees/y", "/api", "y"))
            .listed(worktree("/web", "/web", "main"))
            .build();
        assert_eq!(t.workspaces.len(), 1);
        let ws = &t.workspaces[0];
        assert_eq!(ws.worktrees.len(), 2);
        assert!(ws.worktrees[1].sessions.is_empty());
        assert!(!ws.flat());
    }

    #[test]
    fn workspaces_go_by_name_and_the_main_checkout_comes_first() {
        let b = "/src/web/.claude/worktrees/b";
        let t = tree()
            .session(session("w").cwd(b))
            .session(session("m").cwd("/src/web"))
            .session(session("a").cwd("/src/Api"))
            .checkout("/src/web", "/src/web", "main")
            .checkout(b, "/src/web", "b")
            .found("/src/Api", None)
            .build();
        let names: Vec<_> = t.workspaces.iter().map(|w| w.name.as_str()).collect();
        assert_eq!(names, vec!["Api", "web"]);
        assert_eq!(ids(&t.workspaces[1].worktrees[0]), vec!["m"]);
    }

    #[test]
    fn a_session_is_found_with_its_workspace_and_worktree() {
        let t = tree()
            .session(session("a").cwd("/api"))
            .checkout("/api", "/api", "main")
            .build();
        let (ws, wt, s) = t.find("a").unwrap();
        assert_eq!(
            (ws.name.as_str(), wt.worktree.path.as_path(), s.id.as_str()),
            ("api", Path::new("/api"), "a")
        );
        assert!(t.find("b").is_none());
    }

    fn shell_ids(node: &WorktreeNode) -> Vec<u128> {
        node.terminals.iter().map(|s| s.id).collect()
    }

    #[test]
    fn a_worktree_where_only_shells_run_is_listed_with_them() {
        let x = "/api/.claude/worktrees/x";
        let t = tree()
            .session(session("a").cwd("/api"))
            .terminal(shell(1, x))
            .terminal(shell(2, x))
            .checkout("/api", "/api", "main")
            .checkout(x, "/api", "x")
            .build();
        let ws = &t.workspaces[0];
        assert_eq!(ws.worktrees.len(), 2);
        assert!(ws.worktrees[1].sessions.is_empty());
        assert_eq!(shell_ids(&ws.worktrees[1]), [1, 2]);
        assert!(!ws.flat());
    }

    #[test]
    fn a_shell_below_a_worktree_s_top_sits_in_that_worktree_beside_its_sessions() {
        let t = tree()
            .session(session("a").cwd("/api"))
            .terminal(shell(1, "/api/src"))
            .checkout("/api", "/api", "main")
            .found("/api/src", Some(worktree("/api", "/api", "main")))
            .build();
        let ws = &t.workspaces[0];
        assert_eq!(ws.worktrees.len(), 1);
        assert_eq!(ids(&ws.worktrees[0]), ["a"]);
        assert_eq!(shell_ids(&ws.worktrees[0]), [1]);
        assert!(ws.flat());
    }

    #[test]
    fn a_shell_outside_git_makes_its_directory_a_workspace() {
        let t = tree()
            .terminal(shell(1, "/notes"))
            .found("/notes", None)
            .build();
        let ws = &t.workspaces[0];
        assert_eq!(
            (ws.name.as_str(), ws.git, ws.flat()),
            ("notes", false, true)
        );
        assert_eq!(shell_ids(&ws.worktrees[0]), [1]);
    }

    #[test]
    fn a_shell_s_workspace_takes_the_worktrees_listed_for_it() {
        let t = tree()
            .terminal(shell(1, "/api"))
            .checkout("/api", "/api", "main")
            .listed(worktree("/api", "/api", "main"))
            .listed(worktree("/api/w", "/api", "w"))
            .build();
        assert_eq!(t.worktrees().count(), 2);
    }

    #[test]
    fn shells_placed_again_go_to_the_innermost_worktree_holding_them() {
        let x = "/api/.claude/worktrees/x";
        let t = tree()
            .session(session("a").cwd("/api"))
            .session(session("b").cwd(x))
            .terminal(shell(1, "/api"))
            .checkout("/api", "/api", "main")
            .checkout(x, "/api", "x")
            .build();
        // One opened since in the nested worktree, one in a directory no
        // worktree listed holds yet; the first is gone.
        let again = t.with_terminals(&[shell(2, format!("{x}/src")), shell(3, "/web")]);
        let ws = &again.workspaces[0];
        assert!(ws.worktrees[0].terminals.is_empty());
        assert_eq!(shell_ids(&ws.worktrees[1]), [2]);
        assert_eq!(again.workspaces.len(), 1);
    }

    #[test]
    fn a_shell_is_found_with_its_workspace_and_worktree() {
        let t = tree()
            .terminal(shell(1, "/api"))
            .checkout("/api", "/api", "main")
            .build();
        let (ws, wt, s) = t.terminal(1).unwrap();
        assert_eq!(
            (ws.name.as_str(), wt.worktree.path.as_path(), s.id),
            ("api", Path::new("/api"), 1)
        );
        assert!(t.terminal(2).is_none());
    }

    #[test]
    fn iterating_sessions_names_each_ones_workspace() {
        let t = tree()
            .session(session("a").cwd("/api"))
            .session(session("b").cwd("/notes"))
            .checkout("/api", "/api", "main")
            .build();
        let got: Vec<_> = t.sessions().map(|(w, s)| (w, s.id.as_str())).collect();
        assert_eq!(got, vec![("api", "a"), ("notes", "b")]);
    }

    const SCRATCH: &str = "/data/adroit/scratch";

    #[test]
    fn sessions_in_directories_under_the_scratch_root_share_one_flat_scratch_workspace() {
        let t = tree()
            .session(session("a").cwd("/data/adroit/scratch/20261019-2130"))
            .session(session("b").cwd("/data/adroit/scratch/20261019-2200"))
            .scratch(SCRATCH)
            .build();
        assert_eq!(t.workspaces.len(), 1);
        let ws = &t.workspaces[0];
        assert_eq!(
            (ws.name.as_str(), ws.root.as_path(), ws.scratch, ws.git),
            ("Scratch", Path::new(SCRATCH), true, false)
        );
        assert!(ws.flat());
        assert_eq!(ids(&ws.worktrees[0]), ["a", "b"]);
        assert_eq!(ws.worktrees[0].worktree.path, PathBuf::from(SCRATCH));
    }

    #[test]
    fn a_scratch_directory_belongs_to_the_scratch_workspace_whatever_git_says() {
        let dir = "/data/adroit/scratch/20261019-2130";
        let t = tree()
            .session(session("a").cwd(dir))
            .found(dir, Some(worktree("/data", "/data", "main")))
            .listed(worktree("/data", "/data", "main"))
            .listed(worktree("/data/w", "/data", "w"))
            .scratch(SCRATCH)
            .build();
        assert_eq!(t.workspaces.len(), 1);
        assert!(t.workspaces[0].scratch);
    }

    #[test]
    fn a_shell_in_the_scratch_root_or_below_it_sits_in_the_scratch_workspace() {
        let dir = "/data/adroit/scratch/20261019-2130";
        let t = tree()
            .session(session("a").cwd(dir))
            .terminal(shell(1, SCRATCH))
            .terminal(shell(2, dir))
            .scratch(SCRATCH)
            .build();
        assert_eq!(t.workspaces.len(), 1);
        assert_eq!(shell_ids(&t.workspaces[0].worktrees[0]), [1, 2]);
    }

    #[test]
    fn a_directory_beside_the_scratch_root_is_not_in_it() {
        let t = tree()
            .session(session("a").cwd("/data/adroit/scratch-old"))
            .scratch(SCRATCH)
            .build();
        assert!(!t.workspaces[0].scratch);
        assert_eq!(t.workspaces[0].name, "scratch-old");
    }

    #[test]
    fn the_scratch_workspace_comes_after_every_other() {
        let t = tree()
            .session(session("s").cwd("/data/adroit/scratch/20261019-2130"))
            .session(session("z").cwd("/src/zebra"))
            .session(session("a").cwd("/src/api"))
            .scratch(SCRATCH)
            .build();
        let names: Vec<_> = t.workspaces.iter().map(|w| w.name.as_str()).collect();
        assert_eq!(names, ["api", "zebra", "Scratch"]);
    }
}
