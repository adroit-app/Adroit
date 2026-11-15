//! What the tree shows of the work: the sessions and terminals there are
//! and the worktrees they run in, what git says of the files of each
//! worktree whose files are listed, and what each directory listed holds.

use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;

use adroit_model::files::Entry;
use adroit_model::github::{self, Listing};
use adroit_model::repository::Marks;
use adroit_model::{Interrupted, Session, TerminalSession, Tree};
use serde::{Deserialize, Serialize};

/// What the tree shows that the host keeps read while it is shown. The
/// sessions are listed whatever the tree shows.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct TreeSight {
    /// Whether every worktree git knows of is listed, those where nothing
    /// runs too.
    pub every_worktree: bool,
    /// The worktrees whose files are listed, whose marks are read.
    pub marked: BTreeSet<PathBuf>,
    /// The directories whose entries are listed.
    pub listed: BTreeSet<PathBuf>,
    /// The repositories, by their root, whose listings on GitHub are open.
    #[serde(default, skip_serializing_if = "BTreeSet::is_empty")]
    pub github: BTreeSet<(PathBuf, Listing)>,
}

/// What the host read for the tree.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct TreeWork {
    /// The sessions and terminals as last listed; none before the first
    /// list.
    pub listed: Option<Box<Listed>>,
    /// Why the sessions cannot be listed, while they cannot.
    pub failing: Option<String>,
    /// By the worktree.
    pub marks: BTreeMap<PathBuf, Marks>,
    /// By the directory.
    pub entries: BTreeMap<PathBuf, Vec<Entry>>,
    /// What GitHub holds open of each repository on it, by its root.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub github: BTreeMap<PathBuf, github::Repository>,
}

/// Every session and terminal there is, and the tree they are placed in by
/// the worktree each runs in. On the wire the tree holds its terminals by
/// `terminals` alone, which carry their ids.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(from = "WireListed", into = "WireListed")]
pub struct Listed {
    /// As listed, ahead of being placed.
    pub sessions: Vec<Session>,
    pub terminals: Vec<TerminalSession>,
    pub tree: Tree,
    /// What the terminal host took with it, waiting to be brought back.
    pub interrupted: Vec<Interrupted>,
}

#[derive(Serialize, Deserialize)]
struct WireListed {
    sessions: Vec<Session>,
    #[serde(with = "crate::wire::terminals")]
    terminals: Vec<TerminalSession>,
    tree: Tree,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    interrupted: Vec<Interrupted>,
}

impl From<Listed> for WireListed {
    fn from(listed: Listed) -> Self {
        WireListed {
            sessions: listed.sessions,
            terminals: listed.terminals,
            tree: listed.tree,
            interrupted: listed.interrupted,
        }
    }
}

impl From<WireListed> for Listed {
    fn from(wire: WireListed) -> Self {
        Listed {
            tree: wire.tree.with_terminals(&wire.terminals),
            sessions: wire.sessions,
            terminals: wire.terminals,
            interrupted: wire.interrupted,
        }
    }
}

/// One change to what the host read for the tree.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum TreeChange {
    Listed {
        listed: Box<Listed>,
    },
    /// The sessions cannot be listed for `error`, or can again.
    Failing {
        error: Option<String>,
    },
    Marks {
        worktree: PathBuf,
        marks: Marks,
    },
    /// The host no longer reads the marks of `worktree`.
    MarksLeft {
        worktree: PathBuf,
    },
    Entries {
        dir: PathBuf,
        entries: Vec<Entry>,
    },
    /// The host no longer lists `dir`.
    EntriesLeft {
        dir: PathBuf,
    },
    /// The repository at `repository` is on GitHub as `github` says, or is
    /// not, or `gh` cannot be run there.
    #[serde(rename = "github")]
    GitHub {
        repository: PathBuf,
        github: Option<Box<github::Repository>>,
    },
}

impl TreeWork {
    /// The tree's facts with `change` made; whether that changed them.
    pub fn apply(&mut self, change: TreeChange) -> bool {
        match change {
            TreeChange::Listed { listed } => set(&mut self.listed, Some(listed)),
            TreeChange::Failing { error } => set(&mut self.failing, error),
            TreeChange::Marks { worktree, marks } => {
                self.marks.insert(worktree, marks.clone()) != Some(marks)
            }
            TreeChange::MarksLeft { worktree } => self.marks.remove(&worktree).is_some(),
            TreeChange::Entries { dir, entries } => {
                self.entries.insert(dir, entries.clone()) != Some(entries)
            }
            TreeChange::EntriesLeft { dir } => self.entries.remove(&dir).is_some(),
            TreeChange::GitHub {
                repository,
                github: Some(github),
            } => self.github.insert(repository, (*github).clone()) != Some(*github),
            TreeChange::GitHub {
                repository,
                github: None,
            } => self.github.remove(&repository).is_some(),
        }
    }

    /// The tree as last listed, or an empty one.
    pub fn tree(&self) -> &Tree {
        static EMPTY: Tree = Tree {
            workspaces: Vec::new(),
        };
        self.listed.as_ref().map_or(&EMPTY, |l| &l.tree)
    }

    /// Every session as last listed.
    pub fn sessions(&self) -> &[Session] {
        self.listed.as_ref().map_or(&[], |l| &l.sessions)
    }

    /// Every terminal as last listed.
    pub fn terminals(&self) -> &[TerminalSession] {
        self.listed.as_ref().map_or(&[], |l| &l.terminals)
    }

    /// What the terminal host took with it, as last listed.
    pub fn interrupted(&self) -> &[Interrupted] {
        self.listed.as_ref().map_or(&[], |l| &l.interrupted)
    }
}

/// `slot` holding `value`; whether it held something else.
fn set<T: PartialEq>(slot: &mut T, value: T) -> bool {
    if *slot == value {
        return false;
    }
    *slot = value;
    true
}

#[cfg(test)]
mod tests {
    use adroit_model::testing::session;

    use super::*;
    use crate::testing::listed;

    /// The sessions `ids` listed anew.
    fn listing(ids: &[&str]) -> TreeChange {
        TreeChange::Listed {
            listed: Box::new(listed().sessions(ids.iter().copied().map(session)).build()),
        }
    }

    fn entries(dir: &str, names: &[&str]) -> TreeChange {
        TreeChange::Entries {
            dir: dir.into(),
            entries: names
                .iter()
                .map(|n| Entry {
                    path: PathBuf::from(dir).join(n),
                    dir: false,
                })
                .collect(),
        }
    }

    #[test]
    fn a_change_says_whether_it_changed_anything() {
        let mut work = TreeWork::default();
        // (change, whether it changed the work)
        let cases = [
            (listing(&["a"]), true),
            (listing(&["a"]), false),
            (listing(&[]), true),
            (TreeChange::Failing { error: None }, false),
            (
                TreeChange::Failing {
                    error: Some("no daemon".into()),
                },
                true,
            ),
            (entries("/w", &["a.rs"]), true),
            (entries("/w", &["a.rs"]), false),
            (entries("/w", &["b.rs"]), true),
            (TreeChange::EntriesLeft { dir: "/w".into() }, true),
            (TreeChange::EntriesLeft { dir: "/w".into() }, false),
            (
                TreeChange::Marks {
                    worktree: "/w".into(),
                    marks: Marks::default(),
                },
                true,
            ),
            (
                TreeChange::MarksLeft {
                    worktree: "/w".into(),
                },
                true,
            ),
        ];
        for (change, changed) in cases {
            assert_eq!(work.apply(change.clone()), changed, "{change:?}");
        }
    }

    #[test]
    fn a_repository_on_github_is_kept_until_it_is_said_to_be_no_more() {
        let mut work = TreeWork::default();
        let on = |slug: &str| TreeChange::GitHub {
            repository: "/r".into(),
            github: Some(Box::new(github::Repository::new(slug))),
        };
        let off = TreeChange::GitHub {
            repository: "/r".into(),
            github: None,
        };
        assert!(work.apply(on("o/r")));
        assert!(!work.apply(on("o/r")));
        assert!(work.apply(on("o/s")));
        assert_eq!(work.github["/r".as_ref() as &std::path::Path].slug, "o/s");
        assert!(work.apply(off.clone()));
        assert!(!work.apply(off));
        assert!(work.github.is_empty());
    }

    #[test]
    fn nothing_listed_yet_is_an_empty_tree() {
        let mut work = TreeWork::default();
        assert_eq!(work.tree(), &Tree::default());
        assert!(work.sessions().is_empty());
        work.apply(listing(&["a", "b"]));
        let ids: Vec<_> = work.sessions().iter().map(|s| s.id.as_str()).collect();
        assert_eq!(ids, ["a", "b"]);
    }

    #[test]
    fn what_the_host_left_is_dropped_whole() {
        let mut work = TreeWork::default();
        work.apply(entries("/w", &["a.rs"]));
        work.apply(entries("/v", &["b.rs"]));
        work.apply(TreeChange::EntriesLeft { dir: "/w".into() });
        assert_eq!(
            work.entries.keys().collect::<Vec<_>>(),
            [&PathBuf::from("/v")]
        );
    }
}
