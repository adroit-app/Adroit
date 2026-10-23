use serde::Serialize;
use serde::de::DeserializeOwned;

/// A line that does not hold the message it was read as.
#[derive(Debug, thiserror::Error)]
#[error("cannot read the message: {0}")]
pub struct DecodeError(#[from] serde_json::Error);

/// `message` as one line of JSON, ending in a newline, as it goes on the
/// wire.
pub fn encode<T: Serialize>(message: &T) -> String {
    let mut line =
        serde_json::to_string(message).expect("a protocol message always serializes to JSON");
    line.push('\n');
    line
}

/// The message one line holds, with or without its newline.
pub fn decode<T: DeserializeOwned>(line: impl AsRef<str>) -> Result<T, DecodeError> {
    Ok(serde_json::from_str(line.as_ref().trim_end_matches('\n'))?)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::{BTreeMap, BTreeSet};
    use std::path::Path;

    use adroit_model::files::Entry;
    use adroit_model::links::Links;
    use adroit_model::repository::Marks;
    use adroit_model::repository::{Listing, Rev, Spot};
    use adroit_model::usage::{Limit, Severity, Window};
    use adroit_model::{
        Past, Session, TerminalSession, Tree, WorkspaceNode, Worktree, WorktreeNode,
    };

    use crate::{
        Change, ConversationRead, DetailsChange, DetailsWork, Event, HistoryRead, Listed, Offers,
        PastChange, PastSight, PastWork, PickChange, PickSight, PickWork, PlaceChange, PlaceRead,
        PlaceWork, PromptChange, PromptWork, RepositoryRead, RepositorySight, Request, Seq,
        SessionRead, Sight, SystemUsage, TreeChange, TreeSight, Version, WorkPatch, WorkSnapshot,
    };

    fn round_trips<T>(message: T)
    where
        T: Serialize + DeserializeOwned + PartialEq + std::fmt::Debug,
    {
        let line = encode(&message);
        assert!(line.ends_with('\n'));
        assert_eq!(line.matches('\n').count(), 1, "one line: {line:?}");
        assert_eq!(decode::<T>(&line).unwrap(), message);
    }

    #[test]
    fn every_request_round_trips() {
        round_trips(Request::Hello {
            versions: vec![Version(1), Version(2)],
        });
        round_trips(Request::Snapshot);
        round_trips(Request::Watch(Box::default()));
        round_trips(Request::ReadAgain);
        round_trips(Request::Watch(Box::new(Sight {
            session: Some("s".into()),
            details: true,
            overview: true,
            repository: Some(RepositorySight {
                worktree: "/w".into(),
                spot: Spot::Files(Rev::Commit("abc".into())),
            }),
            usage: Some(adroit_model::Agent::Codex),
            tree: TreeSight {
                every_worktree: true,
                marked: BTreeSet::from(["/w".into()]),
                listed: BTreeSet::from(["/w".into(), "/w/src".into()]),
            },
            past: Some(PastSight {
                dir: Some("/w".into()),
            }),
            pick: Some(PickSight {
                listed: BTreeSet::from(["/home".into()]),
            }),
            place: Some("/w".into()),
        })));
        round_trips(Request::Relist);
        round_trips(Request::Typed {
            inputs: BTreeMap::from([(u128::MAX, 1_700_000_000_000)]),
        });
        round_trips(Request::TerminalOpened {
            terminal: TerminalSession {
                id: u128::MAX - 1,
                name: "zsh".into(),
                cwd: "/w".into(),
                ..Default::default()
            },
        });
        round_trips(Request::TerminalClosed { id: u128::MAX });
        round_trips(Request::History {
            asked: 3,
            agent: adroit_model::Agent::Claude,
            project: "/w".into(),
        });
    }

    #[test]
    fn every_event_round_trips() {
        round_trips(Event::Welcome {
            version: Version(1),
        });
        round_trips(Event::Refused {
            versions: vec![Version(3)],
        });
        round_trips(Event::Snapshot(Box::new(WorkSnapshot {
            seq: Seq(7),
            details: DetailsWork::default(),
            tree: Default::default(),
            past: Default::default(),
            pick: Default::default(),
            place: Default::default(),
            prompt: Default::default(),
        })));
        round_trips(Event::Snapshot(Box::new(WorkSnapshot {
            seq: Seq(7),
            details: DetailsWork {
                sessions: BTreeMap::from([(
                    "s".to_string(),
                    SessionRead {
                        links: Some(Links::default()),
                        thumbs: BTreeMap::from([("/w/a.png".to_string(), Some("data:".into()))]),
                        ..Default::default()
                    },
                )]),
                usage: BTreeMap::from([(adroit_model::Agent::Claude, Vec::new())]),
                system: Some(SystemUsage::default()),
                ..Default::default()
            },
            tree: Default::default(),
            past: PastWork {
                listed: Some((
                    PastSight::default(),
                    vec![Past {
                        agent: adroit_model::Agent::Codex,
                        id: "019f7a4b".into(),
                        cwd: "/w".into(),
                        title: "review".into(),
                        at_ms: 1,
                    }],
                )),
            },
            pick: PickWork {
                offers: Some(Offers::default()),
            },
            place: PlaceWork {
                read: Some(PlaceRead::default()),
            },
            prompt: PromptWork {
                history: Some(HistoryRead::default()),
            },
        })));
        round_trips(Event::Patch(WorkPatch {
            seq: Seq(8),
            changes: vec![],
        }));
        let details = |change| Change::Details(change);
        let shell = TerminalSession {
            id: u128::MAX,
            name: "zsh".into(),
            cwd: "/w/src".into(),
            ..Default::default()
        };
        round_trips(Event::Patch(WorkPatch {
            seq: Seq(9),
            changes: vec![
                details(DetailsChange::Conversation {
                    session: "s".into(),
                    conversation: ConversationRead::default(),
                }),
                details(DetailsChange::Repository {
                    worktree: "/w".into(),
                    read: RepositoryRead {
                        spot: Spot::Log,
                        listing: Err("not a repository".into()),
                        revision: None,
                    },
                }),
                details(DetailsChange::Repository {
                    worktree: "/w".into(),
                    read: RepositoryRead {
                        spot: Spot::Log,
                        listing: Ok(Listing::Log(Vec::new())),
                        revision: None,
                    },
                }),
                details(DetailsChange::Usage {
                    agent: adroit_model::Agent::Claude,
                    limits: vec![Limit {
                        window: Window::WeekOf("Opus".into()),
                        percent: 12.5,
                        resets_ms: Some(1),
                        severity: Severity::Warning,
                    }],
                }),
                details(DetailsChange::SessionLeft {
                    session: "s".into(),
                }),
                Change::Tree(TreeChange::Listed {
                    listed: Box::new(Listed {
                        sessions: vec![Session {
                            id: "s".into(),
                            ..Default::default()
                        }],
                        terminals: vec![shell.clone()],
                        tree: Tree {
                            workspaces: vec![WorkspaceNode {
                                root: "/w".into(),
                                name: "w".into(),
                                git: true,
                                scratch: false,
                                worktrees: vec![WorktreeNode {
                                    worktree: Worktree::plain("/w"),
                                    sessions: Vec::new(),
                                    terminals: vec![shell.clone()],
                                }],
                            }],
                        },
                    }),
                }),
                Change::Tree(TreeChange::Failing {
                    error: Some("no daemon".into()),
                }),
                Change::Tree(TreeChange::Marks {
                    worktree: "/w".into(),
                    marks: Marks::new(
                        Path::new("/w"),
                        &[adroit_model::repository::Change {
                            status: " M".into(),
                            path: "a.rs".into(),
                            from: None,
                        }],
                        &["target/".into()],
                        false,
                    ),
                }),
                Change::Tree(TreeChange::Entries {
                    dir: "/w".into(),
                    entries: vec![Entry {
                        path: "/w/a.rs".into(),
                        dir: false,
                    }],
                }),
                Change::Tree(TreeChange::EntriesLeft { dir: "/w".into() }),
                Change::Tree(TreeChange::MarksLeft {
                    worktree: "/w".into(),
                }),
                Change::Past(PastChange::Listed {
                    sight: PastSight {
                        dir: Some("/w".into()),
                    },
                    conversations: Vec::new(),
                }),
                Change::Past(PastChange::Left),
                Change::Pick(PickChange::Offers {
                    offers: Offers {
                        recent: vec!["/w".into()],
                        ghq: vec!["/g/r".into()],
                        listed: vec!["/l/a".into()],
                        roots: vec!["/g".into(), "/l".into()],
                    },
                }),
                Change::Pick(PickChange::Left),
                Change::Place(PlaceChange::Read {
                    read: PlaceRead {
                        worktree: "/w".into(),
                        files: vec!["src/a.rs".into()],
                        jj: true,
                    },
                }),
                Change::Place(PlaceChange::Left),
                Change::Prompt(PromptChange::History {
                    read: HistoryRead {
                        asked: 1,
                        project: "/w".into(),
                        sent: vec!["fix".into()],
                    },
                }),
            ],
        }));
    }

    #[test]
    fn the_handshake_reads_the_same_in_every_version() {
        // A peer reads the handshake before it knows which version the other
        // speaks, so its shape must never change.
        let cases = [
            (
                encode(&Request::Hello {
                    versions: vec![Version(1)],
                }),
                "{\"type\":\"hello\",\"versions\":[1]}\n",
            ),
            (
                encode(&Event::Welcome {
                    version: Version(1),
                }),
                "{\"type\":\"welcome\",\"version\":1}\n",
            ),
            (
                encode(&Event::Refused {
                    versions: vec![Version(2)],
                }),
                "{\"type\":\"refused\",\"versions\":[2]}\n",
            ),
        ];
        for (line, want) in cases {
            assert_eq!(line, want);
        }
    }

    #[test]
    fn a_line_that_is_not_a_message_is_an_error() {
        assert!(decode::<Request>("{\"type\":\"no_such\"}").is_err());
        assert!(decode::<Event>("not json").is_err());
    }
}
