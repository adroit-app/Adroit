//! Builders for the host's words tests feed a window: the sessions listed,
//! the snapshot of the work and the patches after it, shared by every
//! crate's tests through the `testing` feature.
//!
//! Each builder starts from the plainest value and names only what a test
//! sets, so a part added to [`WorkSnapshot`] changes this file alone.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use adroit_model::testing::worktree;
use adroit_model::{Session, TerminalSession, Worktree, build};

use crate::{
    Change, DetailsWork, Event, Listed, Seq, TreeChange, TreeWork, WorkPatch, WorkSnapshot,
};

/// The repository [`listed`] places every directory in unless told
/// otherwise.
pub const REPO: &str = "/r";

/// Nothing listed yet; each directory a session or shell runs in is placed
/// as a worktree of [`REPO`] on `main`, from its top.
pub fn listed() -> ListedBuilder {
    ListedBuilder {
        repo: Some(PathBuf::from(REPO)),
        ..ListedBuilder::default()
    }
}

/// A [`Listed`] being built; see [`listed`].
#[derive(Debug, Clone, Default)]
#[must_use]
pub struct ListedBuilder {
    sessions: Vec<Session>,
    terminals: Vec<TerminalSession>,
    repo: Option<PathBuf>,
    found: HashMap<PathBuf, Option<Worktree>>,
    worktrees: Vec<Worktree>,
}

impl ListedBuilder {
    pub fn session(mut self, session: impl Into<Session>) -> Self {
        self.sessions.push(session.into());
        self
    }

    pub fn sessions<S: Into<Session>>(mut self, sessions: impl IntoIterator<Item = S>) -> Self {
        self.sessions.extend(sessions.into_iter().map(Into::into));
        self
    }

    pub fn terminal(mut self, terminal: TerminalSession) -> Self {
        self.terminals.push(terminal);
        self
    }

    pub fn terminals(mut self, terminals: impl IntoIterator<Item = TerminalSession>) -> Self {
        self.terminals.extend(terminals);
        self
    }

    /// Every directory not [`found`](Self::found) placed in the repository
    /// at `repo` instead of [`REPO`].
    pub fn repo(mut self, repo: impl Into<PathBuf>) -> Self {
        self.repo = Some(repo.into());
        self
    }

    /// Every directory not [`found`](Self::found) outside git.
    pub fn outside_git(mut self) -> Self {
        self.repo = None;
        self
    }

    /// What git said of `dir`: the worktree holding it, or `None` outside
    /// git.
    pub fn found(mut self, dir: impl Into<PathBuf>, worktree: Option<Worktree>) -> Self {
        self.found.insert(dir.into(), worktree);
        self
    }

    /// A worktree git lists, whether anything runs in it or not.
    pub fn worktree(mut self, worktree: Worktree) -> Self {
        self.worktrees.push(worktree);
        self
    }

    pub fn build(self) -> Listed {
        let mut found = self.found;
        if let Some(repo) = &self.repo {
            let dirs = self.sessions.iter().map(|s| &s.cwd);
            for dir in dirs.chain(self.terminals.iter().map(|t| &t.cwd)) {
                found
                    .entry(dir.clone())
                    .or_insert_with(|| Some(worktree(dir.clone(), repo.clone(), "main")));
            }
        }
        let tree = build(
            &self.sessions,
            &self.terminals,
            &found,
            &self.worktrees,
            None::<&Path>,
        );
        Listed {
            sessions: self.sessions,
            terminals: self.terminals,
            tree,
        }
    }
}

impl From<ListedBuilder> for Listed {
    fn from(b: ListedBuilder) -> Self {
        b.build()
    }
}

/// The host's first word: nothing of the work read, as of the first change.
pub fn snapshot() -> SnapshotBuilder {
    SnapshotBuilder(WorkSnapshot {
        seq: Seq(1),
        ..WorkSnapshot::default()
    })
}

/// A [`WorkSnapshot`] being built; see [`snapshot`].
#[derive(Debug, Clone)]
#[must_use]
pub struct SnapshotBuilder(WorkSnapshot);

impl SnapshotBuilder {
    pub fn seq(mut self, seq: u64) -> Self {
        self.0.seq = Seq(seq);
        self
    }

    /// The sessions and terminals as listed.
    pub fn listed(mut self, listed: impl Into<Listed>) -> Self {
        self.0.tree.listed = Some(Box::new(listed.into()));
        self
    }

    pub fn tree(mut self, tree: TreeWork) -> Self {
        self.0.tree = tree;
        self
    }

    pub fn details(mut self, details: DetailsWork) -> Self {
        self.0.details = details;
        self
    }

    /// Any other part of the work, set by hand.
    pub fn with(mut self, set: impl FnOnce(&mut WorkSnapshot)) -> Self {
        set(&mut self.0);
        self
    }

    pub fn build(self) -> WorkSnapshot {
        self.0
    }
}

impl From<SnapshotBuilder> for WorkSnapshot {
    fn from(b: SnapshotBuilder) -> Self {
        b.build()
    }
}

impl From<SnapshotBuilder> for Event {
    fn from(b: SnapshotBuilder) -> Self {
        Event::Snapshot(Box::new(b.build()))
    }
}

/// The host's `seq`th word, changing nothing yet.
pub fn patch(seq: u64) -> PatchBuilder {
    PatchBuilder(WorkPatch {
        seq: Seq(seq),
        changes: Vec::new(),
    })
}

/// A [`WorkPatch`] being built; see [`patch`].
#[derive(Debug, Clone)]
#[must_use]
pub struct PatchBuilder(WorkPatch);

impl PatchBuilder {
    pub fn change(mut self, change: impl Into<Change>) -> Self {
        self.0.changes.push(change.into());
        self
    }

    pub fn changes<C: Into<Change>>(mut self, changes: impl IntoIterator<Item = C>) -> Self {
        self.0.changes.extend(changes.into_iter().map(Into::into));
        self
    }

    /// The sessions and terminals listed anew.
    pub fn listed(self, listed: impl Into<Listed>) -> Self {
        self.change(TreeChange::Listed {
            listed: Box::new(listed.into()),
        })
    }

    pub fn build(self) -> WorkPatch {
        self.0
    }
}

impl From<PatchBuilder> for WorkPatch {
    fn from(b: PatchBuilder) -> Self {
        b.build()
    }
}

impl From<PatchBuilder> for Event {
    fn from(b: PatchBuilder) -> Self {
        Event::Patch(b.build())
    }
}

#[cfg(test)]
mod tests {
    use adroit_model::testing::{session, shell};

    use super::*;

    #[test]
    fn each_directory_listed_is_placed_in_the_repository_unless_found_otherwise() {
        let l = listed()
            .session(session("a").cwd("/r/a"))
            .terminal(shell(1, "/r/b"))
            .found("/x", None)
            .session(session("x").cwd("/x"))
            .build();
        let found = HashMap::from([
            ("/r/a".into(), Some(worktree("/r/a", REPO, "main"))),
            ("/r/b".into(), Some(worktree("/r/b", REPO, "main"))),
            ("/x".into(), None),
        ]);
        assert_eq!(l.tree, build(&l.sessions, &l.terminals, &found, &[], None));
        assert_eq!(l.sessions.len(), 2);
        assert_eq!(l.terminals.len(), 1);
    }

    #[test]
    fn a_snapshot_starts_at_the_first_change_and_a_patch_at_its_own() {
        let Event::Snapshot(work) = Event::from(snapshot().listed(listed())) else {
            panic!("not a snapshot");
        };
        assert_eq!(work.seq, Seq(1));
        assert_eq!(work.tree.listed.as_deref(), Some(&listed().build()));
        let p = patch(3).listed(listed()).build();
        assert_eq!(p.seq, Seq(3));
        assert_eq!(
            p.changes,
            [Change::Tree(TreeChange::Listed {
                listed: Box::new(listed().build())
            })]
        );
    }
}
