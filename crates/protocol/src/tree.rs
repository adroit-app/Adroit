//! What the tree shows of the work: the sessions and terminals there are
//! and the worktrees they run in, what git says of the files of each
//! worktree whose files are listed, and what each directory listed holds.

use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;

use adroit_model::files::Entry;
use adroit_model::repository::Marks;
use adroit_model::{Session, TerminalSession, Tree};
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
}

/// Every session and terminal there is, and the tree they are placed in by
/// the worktree each runs in. On the wire the tree holds its terminals by
/// `terminals` alone, which carry their ids.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(from = "WireListed", into = "WireListed")]
pub struct Listed {
    /// As listed, ahead of being placed: those the terminal host runs, then
    /// those running in other terminals.
    pub sessions: Vec<Session>,
    pub terminals: Vec<TerminalSession>,
    pub tree: Tree,
}

#[derive(Serialize, Deserialize)]
struct WireListed {
    sessions: Vec<Session>,
    #[serde(with = "crate::wire::terminals")]
    terminals: Vec<TerminalSession>,
    tree: Tree,
}

impl From<Listed> for WireListed {
    fn from(listed: Listed) -> Self {
        WireListed {
            sessions: listed.sessions,
            terminals: listed.terminals,
            tree: listed.tree,
        }
    }
}

impl From<WireListed> for Listed {
    fn from(wire: WireListed) -> Self {
        Listed {
            tree: wire.tree.with_terminals(&wire.terminals),
            sessions: wire.sessions,
            terminals: wire.terminals,
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
    use super::*;

    fn listed(names: &[&str]) -> TreeChange {
        TreeChange::Listed {
            listed: Box::new(Listed {
                sessions: names
                    .iter()
                    .map(|n| Session {
                        id: (*n).into(),
                        ..Default::default()
                    })
                    .collect(),
                ..Default::default()
            }),
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
            (listed(&["a"]), true),
            (listed(&["a"]), false),
            (listed(&[]), true),
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
    fn nothing_listed_yet_is_an_empty_tree() {
        let mut work = TreeWork::default();
        assert_eq!(work.tree(), &Tree::default());
        assert!(work.sessions().is_empty());
        work.apply(listed(&["a", "b"]));
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
