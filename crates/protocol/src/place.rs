//! Where the work is, by the host it is on, and what the host reads of the
//! worktree in the middle of a window: the files `@` in the prompt offers,
//! and whether jj keeps it.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::work::set;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PathEntry {
    pub path: String,
    pub directory: bool,
}

/// The name of a host the work is on. Every place is on the work host's
/// own machine for now, which is named [`HostName::local`].
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(transparent)]
pub struct HostName(pub String);

impl HostName {
    /// The machine the work host runs on.
    pub fn local() -> Self {
        Self("local".into())
    }

    pub fn is_local(&self) -> bool {
        *self == Self::local()
    }
}

impl Default for HostName {
    fn default() -> Self {
        Self::local()
    }
}

/// A directory or a file, on the host it is on.
#[derive(Debug, Clone, Default, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct HostPath {
    pub host: HostName,
    pub path: PathBuf,
}

impl HostPath {
    /// `path` on the work host's own machine.
    pub fn local(path: impl Into<PathBuf>) -> Self {
        Self {
            host: HostName::local(),
            path: path.into(),
        }
    }

    /// The path, when it is on the work host's own machine.
    pub fn on_local(&self) -> Option<&Path> {
        self.host.is_local().then_some(self.path.as_path())
    }
}

/// What the host read of one worktree.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct PlaceRead {
    pub worktree: HostPath,
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
        set(&mut self.read, now)
    }

    /// What was read of `worktree`, on the work host's own machine; none
    /// while it is not.
    pub fn of(&self, worktree: &Path) -> Option<&PlaceRead> {
        self.read
            .as_ref()
            .filter(|r| r.worktree.on_local() == Some(worktree))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn read(worktree: &str, files: &[&str]) -> PlaceChange {
        PlaceChange::Read {
            read: PlaceRead {
                worktree: HostPath::local(worktree),
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

    #[test]
    fn a_place_is_named_by_its_host_and_one_on_another_is_not_the_local_one() {
        let local = HostPath::local("/w");
        assert_eq!(
            crate::encode(&local),
            "{\"host\":\"local\",\"path\":\"/w\"}\n"
        );
        assert_eq!(local.on_local(), Some(Path::new("/w")));
        let away = HostPath {
            host: HostName("build-box".into()),
            path: "/w".into(),
        };
        assert_eq!(away.on_local(), None);
        let mut work = PlaceWork::default();
        work.apply(PlaceChange::Read {
            read: PlaceRead {
                worktree: away,
                ..Default::default()
            },
        });
        assert_eq!(work.of("/w".as_ref()), None);
    }
}
