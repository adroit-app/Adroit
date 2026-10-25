//! What the details panel shows of the work: what the host reads while a
//! window shows it, kept by the session or worktree it was read for, and
//! the changes that bring a window's copy up to date.

use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;
use std::sync::Arc;

use adroit_model::links::Links;
use adroit_model::repository::{Listing, Revision, Spot};
use adroit_model::usage::Limit;
use adroit_model::{Agent, ConversationStats, Note, Process};
use serde::{Deserialize, Serialize};

use crate::{CommandsSight, HostPath, PastSight, PickSight, TreeSight};

/// What a window shows that the host keeps read while it is shown. The host
/// reads nothing for a window that shows nothing.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Sight {
    /// The session shown, by its id, whose conversation and processes the
    /// details read.
    pub session: Option<String>,
    /// Whether the details panel is open.
    pub details: bool,
    /// Whether the details panel is on a sheet that lists the shown
    /// session's entries: its overview, agents or resources. Named for the
    /// overview the sheets were split from, as the wire still calls it.
    pub overview: bool,
    /// The worktree shown and what of its repository the panel lists.
    pub repository: Option<RepositorySight>,
    /// The account whose usage the panel shows.
    pub usage: Option<Account>,
    /// What the tree lists.
    pub tree: TreeSight,
    /// The conversations the agents kept that are listed, while they are.
    pub past: Option<PastSight>,
    /// What the picker lists, while it is open.
    pub pick: Option<PickSight>,
    /// The worktree in the middle, whose files `@` in the prompt offers.
    pub place: Option<HostPath>,
    /// The agent and the directory whose commands `/` in the prompt offers.
    pub commands: Option<CommandsSight>,
    /// The terminals the window's tabs show, in any of their windows.
    #[serde(with = "crate::wire::wide_set")]
    pub terminals: BTreeSet<u128>,
}

impl Sight {
    /// Every directory whose entries are listed, for the tree or the
    /// picker.
    pub fn listed_dirs(&self) -> BTreeSet<PathBuf> {
        let picked = self.pick.iter().flat_map(|p| p.listed.iter());
        self.tree.listed.iter().chain(picked).cloned().collect()
    }
}

/// An agent's account: the one it is logged in as in a login, the agent's
/// own environment or a folder of a profile's own.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct Account {
    pub agent: Agent,
    /// The folder of a profile's own the login is kept in; none for the
    /// agent's own environment.
    pub home: Option<PathBuf>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RepositorySight {
    pub worktree: PathBuf,
    pub spot: Spot,
}

/// What the host read for the details, by what it was read for.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct DetailsWork {
    /// By the session's id.
    pub sessions: BTreeMap<String, SessionRead>,
    /// By the worktree.
    pub repositories: BTreeMap<PathBuf, RepositoryRead>,
    /// Each account's usage limits.
    #[serde(with = "crate::wire::pairs")]
    pub usage: BTreeMap<Account, Vec<Limit>>,
    /// The whole machine's usage.
    pub system: Option<SystemUsage>,
}

/// What was read of one session.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct SessionRead {
    pub conversation: Option<ConversationRead>,
    /// How full the conversation's context is, as the agent was asked,
    /// for an agent whose transcript does not say.
    pub context: Option<ContextRead>,
    pub links: Option<Links>,
    /// Small copies of the image files among its links, by path; `None`
    /// for a file that cannot be made small here.
    pub thumbs: BTreeMap<String, Option<Arc<str>>>,
    /// What it sent and received through Adroit.
    pub notes: Option<Vec<Note>>,
    pub processes: Option<Vec<Process>>,
}

/// What a conversation says of itself, with the conversation's id, which
/// changes when the session goes on in another conversation.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ConversationRead {
    pub id: String,
    pub stats: ConversationStats,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ContextRead {
    /// The conversation it was asked of.
    pub conversation: String,
    /// The tokens it holds and the most it can.
    pub context: (u64, u64),
}

/// What was read of a repository for a spot of the panel.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RepositoryRead {
    pub spot: Spot,
    /// What the spot lists, or why it could not be read.
    pub listing: Result<Listing, String>,
    /// The revision whose files it lists.
    pub revision: Option<Revision>,
}

/// The whole machine's usage, each in percent with a note on what it is of.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct SystemUsage {
    pub cpu: Option<f32>,
    pub gpu: Option<f32>,
    pub memory: Option<f32>,
    pub storage: Option<f32>,
    pub cpu_note: String,
    pub gpu_note: String,
    pub memory_note: String,
    pub storage_note: String,
}

/// One change to what the host read for the details. Each names the session,
/// worktree or agent it was read for and carries only what changed of it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum DetailsChange {
    Conversation {
        session: String,
        conversation: ConversationRead,
    },
    Context {
        session: String,
        context: ContextRead,
    },
    Links {
        session: String,
        links: Links,
    },
    Thumbs {
        session: String,
        thumbs: BTreeMap<String, Option<Arc<str>>>,
    },
    Notes {
        session: String,
        notes: Vec<Note>,
    },
    Processes {
        session: String,
        processes: Vec<Process>,
    },
    /// The host no longer reads `session`; what it read is gone.
    SessionLeft {
        session: String,
    },
    Repository {
        worktree: PathBuf,
        read: RepositoryRead,
    },
    /// The host no longer reads the repository at `worktree`.
    RepositoryLeft {
        worktree: PathBuf,
    },
    Usage {
        account: Account,
        limits: Vec<Limit>,
    },
    System {
        usage: SystemUsage,
    },
}

impl DetailsWork {
    /// The details with `change` made; whether that changed them.
    pub fn apply(&mut self, change: DetailsChange) -> bool {
        match change {
            DetailsChange::Conversation {
                session,
                conversation,
            } => set(&mut self.session(session).conversation, Some(conversation)),
            DetailsChange::Context { session, context } => {
                set(&mut self.session(session).context, Some(context))
            }
            DetailsChange::Links { session, links } => {
                set(&mut self.session(session).links, Some(links))
            }
            DetailsChange::Thumbs { session, thumbs } => {
                set(&mut self.session(session).thumbs, thumbs)
            }
            DetailsChange::Notes { session, notes } => {
                set(&mut self.session(session).notes, Some(notes))
            }
            DetailsChange::Processes { session, processes } => {
                set(&mut self.session(session).processes, Some(processes))
            }
            DetailsChange::SessionLeft { session } => self.sessions.remove(&session).is_some(),
            DetailsChange::Repository { worktree, read } => {
                self.repositories.insert(worktree, read.clone()) != Some(read)
            }
            DetailsChange::RepositoryLeft { worktree } => {
                self.repositories.remove(&worktree).is_some()
            }
            DetailsChange::Usage { account, limits } => {
                self.usage.insert(account, limits.clone()) != Some(limits)
            }
            DetailsChange::System { usage } => set(&mut self.system, Some(usage)),
        }
    }

    fn session(&mut self, id: String) -> &mut SessionRead {
        self.sessions.entry(id).or_default()
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
    use adroit_model::repository::Rev;

    use super::*;

    fn conversation(id: &str, turns: u64) -> ConversationRead {
        ConversationRead {
            id: id.into(),
            stats: ConversationStats {
                user_turns: turns,
                ..Default::default()
            },
        }
    }

    #[test]
    fn a_change_replaces_what_it_names_and_leaves_the_rest() {
        let mut work = DetailsWork::default();
        work.apply(DetailsChange::Conversation {
            session: "a".into(),
            conversation: conversation("c1", 1),
        });
        work.apply(DetailsChange::Notes {
            session: "a".into(),
            notes: Vec::new(),
        });
        work.apply(DetailsChange::Conversation {
            session: "a".into(),
            conversation: conversation("c1", 2),
        });
        work.apply(DetailsChange::Processes {
            session: "b".into(),
            processes: Vec::new(),
        });
        let a = &work.sessions["a"];
        assert_eq!(a.conversation, Some(conversation("c1", 2)));
        assert_eq!(a.notes, Some(Vec::new()));
        assert_eq!(a.links, None);
        assert_eq!(work.sessions["b"].processes, Some(Vec::new()));
    }

    #[test]
    fn a_change_says_whether_it_changed_anything() {
        let mut work = DetailsWork::default();
        let links = || DetailsChange::Links {
            session: "a".into(),
            links: Links::default(),
        };
        let left = || DetailsChange::SessionLeft {
            session: "a".into(),
        };
        assert!(work.apply(links()));
        assert!(!work.apply(links()));
        assert!(work.apply(left()));
        assert!(!work.apply(left()));
        let usage = || DetailsChange::Usage {
            account: Account {
                agent: Agent::Claude,
                home: None,
            },
            limits: Vec::new(),
        };
        assert!(work.apply(usage()));
        assert!(!work.apply(usage()));
    }

    #[test]
    fn what_the_host_left_is_dropped_whole() {
        let mut work = DetailsWork::default();
        let read = RepositoryRead {
            spot: Spot::Files(Rev::WorkingCopy),
            listing: Ok(Listing::Files(Vec::new())),
            revision: None,
        };
        work.apply(DetailsChange::Repository {
            worktree: "/w".into(),
            read: read.clone(),
        });
        work.apply(DetailsChange::Repository {
            worktree: "/v".into(),
            read,
        });
        work.apply(DetailsChange::Links {
            session: "a".into(),
            links: Links::default(),
        });
        work.apply(DetailsChange::RepositoryLeft {
            worktree: "/w".into(),
        });
        work.apply(DetailsChange::SessionLeft {
            session: "a".into(),
        });
        assert_eq!(
            work.repositories.keys().collect::<Vec<_>>(),
            [&PathBuf::from("/v")]
        );
        assert!(work.sessions.is_empty());
    }
}
