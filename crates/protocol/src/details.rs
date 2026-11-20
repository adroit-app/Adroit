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

use crate::work::set;
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
    /// The worktree shown and each spot of its repository the panel lists.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub repositories: Vec<RepositorySight>,
    /// The account whose usage the panel shows.
    pub usage: Option<Account>,
    /// Whether the details list the environment's agents, with the usage of
    /// every login its profiles keep. Left off the wire when not, so a
    /// sight reads the same to a host that knows nothing of it.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub agents: bool,
    /// The machine's resources the window shows while the details are
    /// open, which the host samples; the host's own settings name them for
    /// a window that does not say.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub system: Option<adroit_model::config::UsageSettings>,
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
    /// The sessions besides the one shown, by their ids, whose timelines the
    /// details list while they are open. Left off the wire when none, so a
    /// sight reads the same to a host that knows nothing of it.
    #[serde(default, skip_serializing_if = "BTreeSet::is_empty")]
    pub timelines: BTreeSet<String>,
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
    /// By the worktree, one for each spot read.
    pub repositories: BTreeMap<PathBuf, Vec<RepositoryRead>>,
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
    /// The journal of what its agent recorded of its work, oldest first.
    pub timeline: Option<Vec<adroit_model::timeline::Entry>>,
}

/// What a conversation says of itself, with the conversation's id, which
/// changes when the session goes on in another conversation.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ConversationRead {
    pub id: String,
    pub stats: ConversationStats,
    /// What the session did in its latest turns, the latest last, as its
    /// agent's hooks told; none for an agent whose hooks are not read. Left
    /// off the wire when none, and read as none when it cannot be, so a
    /// peer of another build reads the rest all the same.
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "adroit_model::steps::lenient"
    )]
    pub turns: Option<Vec<adroit_model::steps::Turn>>,
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
    Timeline {
        session: String,
        entries: Vec<adroit_model::timeline::Entry>,
    },
    /// The host no longer reads `session`; what it read is gone.
    SessionLeft {
        session: String,
    },
    Repository {
        worktree: PathBuf,
        read: RepositoryRead,
    },
    /// The host no longer reads `spot` of the repository at `worktree`.
    RepositoryLeft {
        worktree: PathBuf,
        spot: Spot,
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
            DetailsChange::Timeline { session, entries } => {
                set(&mut self.session(session).timeline, Some(entries))
            }
            DetailsChange::SessionLeft { session } => self.sessions.remove(&session).is_some(),
            DetailsChange::Repository { worktree, read } => {
                let reads = self.repositories.entry(worktree).or_default();
                match reads.iter_mut().find(|r| r.spot == read.spot) {
                    Some(had) => set(had, read),
                    None => {
                        reads.push(read);
                        true
                    }
                }
            }
            DetailsChange::RepositoryLeft { worktree, spot } => {
                let Some(reads) = self.repositories.get_mut(&worktree) else {
                    return false;
                };
                let had = reads.len();
                reads.retain(|r| r.spot != spot);
                let changed = reads.len() != had;
                if reads.is_empty() {
                    self.repositories.remove(&worktree);
                }
                changed
            }
            DetailsChange::Usage { account, limits } => {
                self.usage.insert(account, limits.clone()) != Some(limits)
            }
            DetailsChange::System { usage } => set(&mut self.system, Some(usage)),
        }
    }

    /// What was read of `spot` of the repository at `worktree`.
    pub fn repository(&self, worktree: &std::path::Path, spot: &Spot) -> Option<&RepositoryRead> {
        self.repositories
            .get(worktree)?
            .iter()
            .find(|r| r.spot == *spot)
    }

    fn session(&mut self, id: String) -> &mut SessionRead {
        self.sessions.entry(id).or_default()
    }
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
            turns: None,
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
    fn a_conversation_read_without_turns_reads_alike_on_either_side_of_the_field() {
        let without = serde_json::to_value(conversation("c1", 1)).unwrap();
        assert!(without.get("turns").is_none(), "{without}");
        let older: ConversationRead = serde_json::from_value(without).unwrap();
        assert_eq!(older, conversation("c1", 1));
    }

    #[test]
    fn a_conversation_s_turns_read_back_as_they_were_written() {
        let read = ConversationRead {
            turns: Some(vec![adroit_model::steps::Turn {
                started_ms: 1,
                ended_ms: None,
                steps: vec![adroit_model::steps::Step::Prompted("Go".into())],
            }]),
            ..conversation("c1", 1)
        };
        let sent = serde_json::to_string(&read).unwrap();
        assert_eq!(
            serde_json::from_str::<ConversationRead>(&sent).unwrap(),
            read
        );
    }

    #[test]
    fn turns_this_build_cannot_read_leave_the_rest_of_the_conversation_readable() {
        let mut sent = serde_json::to_value(conversation("c1", 3)).unwrap();
        sent["turns"] = serde_json::json!([{"began": "later"}]);
        let read: ConversationRead = serde_json::from_value(sent).unwrap();
        assert_eq!(read, conversation("c1", 3));
    }

    #[test]
    fn a_sight_that_lists_no_agents_reads_alike_on_either_side_of_the_field() {
        let without = serde_json::to_value(Sight::default()).unwrap();
        assert!(without.get("agents").is_none());
        let older: Sight = serde_json::from_value(without).unwrap();
        assert!(!older.agents);
        let listing = Sight {
            agents: true,
            ..Sight::default()
        };
        let sent = serde_json::to_value(&listing).unwrap();
        assert_eq!(sent["agents"], serde_json::Value::Bool(true));
        assert_eq!(serde_json::from_value::<Sight>(sent).unwrap(), listing);
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
        let read = |spot| RepositoryRead {
            spot,
            listing: Ok(Listing::Files(Vec::new())),
            revision: None,
        };
        let files = Spot::Files(Rev::WorkingCopy);
        work.apply(DetailsChange::Repository {
            worktree: "/w".into(),
            read: read(files.clone()),
        });
        work.apply(DetailsChange::Repository {
            worktree: "/v".into(),
            read: read(files.clone()),
        });
        work.apply(DetailsChange::Links {
            session: "a".into(),
            links: Links::default(),
        });
        work.apply(DetailsChange::RepositoryLeft {
            worktree: "/w".into(),
            spot: files.clone(),
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

    #[test]
    fn each_spot_of_a_repository_is_kept_and_left_on_its_own() {
        let mut work = DetailsWork::default();
        let read = |spot, files: usize| RepositoryRead {
            spot,
            listing: Ok(Listing::Files(vec![
                adroit_model::repository::Change {
                    status: "M ".into(),
                    path: "a".into(),
                    from: None
                };
                files
            ])),
            revision: None,
        };
        let (files, log) = (Spot::Files(Rev::WorkingCopy), Spot::Log);
        let w = PathBuf::from("/w");
        let repository = |read| DetailsChange::Repository {
            worktree: w.clone(),
            read,
        };
        assert!(work.apply(repository(read(files.clone(), 1))));
        assert!(work.apply(repository(read(log.clone(), 0))));
        assert!(!work.apply(repository(read(log.clone(), 0))));
        assert!(work.apply(repository(read(files.clone(), 2))));
        assert_eq!(work.repository(&w, &files), Some(&read(files.clone(), 2)));
        assert!(work.apply(DetailsChange::RepositoryLeft {
            worktree: w.clone(),
            spot: log.clone(),
        }));
        assert_eq!(work.repository(&w, &log), None);
        assert!(work.repository(&w, &files).is_some());
    }
}
