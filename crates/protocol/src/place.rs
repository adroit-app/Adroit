//! What the host reads of the worktree in the middle of a window: the files
//! `@` in the prompt offers, and whether jj keeps it.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

/// What the host read of one worktree.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct PlaceRead {
    pub worktree: PathBuf,
    /// Its files, by their paths in it.
    pub files: Vec<String>,
    /// Whether it is a jj workspace.
    pub jj: bool,
}

/// What the host read of the worktree in the middle.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct PlaceWork {
    /// None until it is read, and once no worktree is in the middle.
    pub read: Option<PlaceRead>,
}

/// One change to what the host read of the worktree in the middle.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum PlaceChange {
    Read {
        read: PlaceRead,
    },
    /// The host no longer reads it.
    Left,
}

impl PlaceWork {
    /// What was read with `change` made; whether that changed it.
    pub fn apply(&mut self, change: PlaceChange) -> bool {
        let now = match change {
            PlaceChange::Read { read } => Some(read),
            PlaceChange::Left => None,
        };
        if self.read == now {
            return false;
        }
        self.read = now;
        true
    }

    /// What was read of `worktree`; none while it is not.
    pub fn of(&self, worktree: &std::path::Path) -> Option<&PlaceRead> {
        self.read.as_ref().filter(|r| r.worktree == worktree)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn read(worktree: &str, files: &[&str]) -> PlaceChange {
        PlaceChange::Read {
            read: PlaceRead {
                worktree: worktree.into(),
                files: files.iter().map(|f| f.to_string()).collect(),
                jj: false,
            },
        }
    }

    #[test]
    fn a_reading_replaces_the_one_before_and_leaving_drops_it() {
        let mut work = PlaceWork::default();
        assert!(work.apply(read("/a", &["x.rs"])));
        assert!(!work.apply(read("/a", &["x.rs"])));
        assert!(work.apply(read("/b", &["y.rs"])));
        assert_eq!(
            work.of("/b".as_ref()).map(|r| r.files.clone()),
            Some(vec!["y.rs".to_string()])
        );
        assert_eq!(work.of("/a".as_ref()), None);
        assert!(work.apply(PlaceChange::Left));
        assert!(!work.apply(PlaceChange::Left));
        assert_eq!(work.read, None);
    }
}
