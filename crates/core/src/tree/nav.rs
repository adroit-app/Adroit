//! The tree as one column of items: the workspaces, each worktree's headings
//! and what they hold, then the sessions waiting on the user.

use std::collections::{HashMap, HashSet};
use std::path::PathBuf;

use adroit_model::github::Listing;
use adroit_model::{EnvironmentId, Tree};

mod cycle;
mod keep;
mod opens;
mod query;
mod rows;
#[cfg(test)]
mod tests;

pub use super::Place;
pub use cycle::*;
pub use keep::*;
pub use opens::*;
pub use query::*;
pub use rows::*;

/// A row of the tree, by the environment it belongs to and its own identity
/// there.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Item {
    /// An environment, heading its work while the window shows more than
    /// one.
    Environment(EnvironmentId),
    /// A session waiting on the user, listed in the inbox.
    Waiting(EnvironmentId, String),
    /// A workspace, by the repository or directory it is.
    Workspace(EnvironmentId, PathBuf),
    Worktree(EnvironmentId, PathBuf),
    /// A heading of a repository, by its root, holding the rows of one kind
    /// from every worktree of it.
    Across(EnvironmentId, PathBuf, Heading),
    /// A row of what runs, listed again under its repository's heading.
    /// Boxed rather than flattened so that every row stands for what its own
    /// row does through [`Item::own`].
    Gathered(Box<Item>),
    /// A heading of a worktree, holding the rows of one kind.
    Heading(EnvironmentId, PathBuf, Heading),
    Session(EnvironmentId, String),
    /// A shell or a Neovim adroit runs, by the daemon's id.
    Terminal(EnvironmentId, u128),
    /// A file or a directory of a worktree, under its Files heading.
    File {
        environment: EnvironmentId,
        worktree: PathBuf,
        path: PathBuf,
        dir: bool,
    },
    /// What is open on GitHub of a repository, by its root.
    GitHub(EnvironmentId, PathBuf, Listing),
    /// A pull request or an issue open on GitHub, by its number.
    GitHubItem(EnvironmentId, PathBuf, Listing, u64),
    /// Why what is open on GitHub could not be read.
    GitHubFailed(EnvironmentId, PathBuf, Listing),
}

/// What a worktree's heading holds.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Heading {
    Files,
    Sessions,
    Terminals,
}

impl Heading {
    pub const ALL: [Heading; 3] = [Heading::Files, Heading::Sessions, Heading::Terminals];

    pub fn name(self) -> &'static str {
        match self {
            Heading::Files => "Files",
            Heading::Sessions => "Sessions",
            Heading::Terminals => "Terminals",
        }
    }
}

impl Item {
    pub fn environment(&self) -> &EnvironmentId {
        match self {
            Item::Environment(environment)
            | Item::Waiting(environment, _)
            | Item::Workspace(environment, _)
            | Item::Worktree(environment, _)
            | Item::Across(environment, ..)
            | Item::Heading(environment, ..)
            | Item::Session(environment, _)
            | Item::Terminal(environment, _)
            | Item::File { environment, .. }
            | Item::GitHub(environment, ..)
            | Item::GitHubItem(environment, ..)
            | Item::GitHubFailed(environment, ..) => environment,
            Item::Gathered(own) => own.environment(),
        }
    }

    /// The row this one is a second row of, or else itself.
    pub fn own(&self) -> &Item {
        match self {
            Item::Gathered(own) => own,
            _ => self,
        }
    }

    /// The key used to remember whether a node is folded.
    pub fn fold_key(&self) -> Option<String> {
        let key = match self {
            Item::Environment(_) => "environment".into(),
            Item::Workspace(_, root) => format!("ws:{}", root.display()),
            Item::Worktree(_, path) => format!("wt:{}", path.display()),
            Item::Across(_, root, heading) => format!("across-{heading:?}:{}", root.display()),
            Item::Heading(_, path, heading) => format!("{heading:?}:{}", path.display()),
            Item::GitHub(_, root, listing) => format!("github-{listing:?}:{}", root.display()),
            Item::File {
                worktree,
                path,
                dir: true,
                ..
            } => format!("dir:{}:{}", worktree.display(), path.display()),
            _ => return None,
        };
        Some(format!("{}@{key}", self.environment()))
    }

    /// Whether the node shows the rows under it before it is unfolded: a
    /// worktree's files and what is open on GitHub are read only once asked
    /// for, and a worktree of a repository would repeat the rows its
    /// repository's headings list.
    fn starts_open(&self) -> bool {
        !matches!(
            self,
            Item::Worktree(..)
                | Item::Heading(_, _, Heading::Files)
                | Item::File { .. }
                | Item::GitHub(..)
        )
    }
}

/// Which nodes of the tree are folded: those the user folded or unfolded,
/// each as they left it, and the rest as they start.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Folds {
    /// Whether each node the user folded or unfolded is open. Kept as the
    /// user left it rather than against how the node starts, since a
    /// workspace starts otherwise once something runs in it or stops.
    chosen: HashMap<String, bool>,
    /// The workspaces where nothing runs, which start folded.
    resting: HashSet<(EnvironmentId, PathBuf)>,
}

impl Folds {
    /// Whether `item` shows the rows under it. A row that does not fold has
    /// none to show.
    pub fn is_open(&self, item: &Item) -> bool {
        item.fold_key().is_some_and(|k| {
            self.chosen
                .get(&k)
                .copied()
                .unwrap_or_else(|| item.starts_open() && !self.rests(item))
        })
    }

    fn rests(&self, item: &Item) -> bool {
        match item {
            Item::Workspace(environment, root) => self
                .resting
                .iter()
                .any(|(e, r)| e == environment && r == root),
            _ => false,
        }
    }

    pub fn set(&mut self, item: &Item, open: bool) {
        if let Some(key) = item.fold_key() {
            self.chosen.insert(key, open);
        }
    }

    /// Notes which workspaces of `environment`'s `tree` hold no session and
    /// no terminal, so that they start folded.
    pub fn rest(&mut self, environment: &EnvironmentId, tree: &Tree) {
        self.resting.retain(|(e, _)| e != environment);
        self.resting.extend(
            tree.workspaces
                .iter()
                .filter(|ws| {
                    ws.worktrees
                        .iter()
                        .all(|w| w.sessions.is_empty() && w.terminals.is_empty())
                })
                .map(|ws| (environment.clone(), ws.root.clone())),
        );
    }

    pub fn toggle(&mut self, item: &Item) {
        let open = self.is_open(item);
        self.set(item, !open);
    }
}
