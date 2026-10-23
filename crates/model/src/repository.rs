//! What a repository says, whichever version control reads it: the files a
//! change touches, the changes of a history, the lines of a diff, which of
//! them the details panel asks for, and the marks the tree gives files.

mod marks;

pub use marks::*;
use serde::{Deserialize, Serialize};

/// A file a change touches.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Change {
    /// How it changed: git's two letters for the worktree, index then
    /// worktree (`M `, ` M`, `??`, …), and one for a commit (`M`, `A`, `D`,
    /// `R`, `C`).
    pub status: String,
    pub path: String,
    /// The path a rename or copy came from.
    pub from: Option<String>,
}

impl Change {
    /// The paths to ask a diff of, the one it came from first.
    pub fn paths(&self) -> Vec<&str> {
        self.from
            .iter()
            .map(String::as_str)
            .chain([self.path.as_str()])
            .collect()
    }

    /// Its path, and where it came from.
    pub fn shown(&self) -> String {
        match &self.from {
            Some(from) => format!("{from} -> {}", self.path),
            None => self.path.clone(),
        }
    }
}

/// A commit, or a jj change.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Commit {
    /// What names it: a commit hash, or a change ID, which a rewrite keeps.
    pub id: String,
    /// `id` as short as it is shown.
    pub short: String,
    /// The first line of its description; empty when it has none.
    pub subject: String,
    /// How long ago, as git or jj words it.
    pub when: String,
    /// The commit hash its children name it by among their parents.
    pub node: String,
    /// The commit hashes of its parents, the first parent first.
    pub parents: Vec<String>,
    /// Whether it is the working copy.
    pub current: bool,
    pub empty: bool,
    pub conflict: bool,
    pub bookmarks: Vec<String>,
}

/// The identity and full description of a single revision.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Revision {
    pub id: String,
    pub short: String,
    pub description: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum DiffKind {
    File,
    Hunk,
    Added,
    Removed,
    Context,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DiffLine {
    pub kind: DiffKind,
    pub text: String,
}

/// Where changes are read from.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum Rev {
    /// What is being worked on: git's worktree, or jj's `@`.
    WorkingCopy,
    /// A commit, or a jj change, by its [`Commit::id`].
    Commit(String),
}

/// What a level of a sheet shows.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum Spot {
    Files(Rev),
    Diff(Rev, Change),
    Log,
}

/// What was read for a spot.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum Listing {
    Files(Vec<Change>),
    Log(Vec<Commit>),
    Diff(Vec<DiffLine>),
}

#[cfg(test)]
mod tests {
    use super::*;

    fn change(status: &str, path: &str, from: Option<&str>) -> Change {
        Change {
            status: status.into(),
            path: path.into(),
            from: from.map(Into::into),
        }
    }

    #[test]
    fn a_renamed_file_is_shown_with_where_it_came_from_and_diffed_on_both_paths() {
        let c = change("R", "new.rs", Some("old.rs"));
        assert_eq!(c.shown(), "old.rs -> new.rs");
        assert_eq!(c.paths(), ["old.rs", "new.rs"]);
        assert_eq!(change("M", "a.rs", None).paths(), ["a.rs"]);
    }
}
