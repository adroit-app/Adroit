//! The work host: the work it holds, how it answers a window's requests,
//! and the link a window reaches it through.

use std::sync::{Arc, Mutex};

use adroit_protocol::{
    Change, DetailsChange, DetailsWork, Event, Request, SPOKEN, Seq, Sight, WorkPatch,
    WorkSnapshot, agree,
};
use tokio::sync::{mpsc, watch};

/// Everything the host holds of the work.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Work {
    seq: Seq,
    details: DetailsWork,
}

impl Work {
    /// The work as a window is first shown it.
    pub fn snapshot(&self) -> WorkSnapshot {
        WorkSnapshot {
            seq: self.seq,
            details: self.details.clone(),
        }
    }

    pub fn details(&self) -> &DetailsWork {
        &self.details
    }
}

#[derive(Debug)]
pub struct Host {
    work: Work,
    /// What the window shows, which the host keeps read.
    sight: watch::Sender<Sight>,
    /// Raised each time the window asks for everything read again.
    again: watch::Sender<u64>,
}

impl Default for Host {
    fn default() -> Self {
        Self::new(Work::default())
    }
}

impl Host {
    pub fn new(work: Work) -> Self {
        Self {
            work,
            sight: watch::Sender::new(Sight::default()),
            again: watch::Sender::new(0),
        }
    }

    pub fn work(&self) -> &Work {
        &self.work
    }

    /// What the window shows, as it changes.
    pub fn sight(&self) -> watch::Receiver<Sight> {
        self.sight.subscribe()
    }

    /// Each time the window asks for everything read again.
    pub fn again(&self) -> watch::Receiver<u64> {
        self.again.subscribe()
    }

    /// What the host tells the window that asked `request`.
    pub fn handle(&mut self, request: Request) -> Vec<Event> {
        match request {
            Request::Hello { versions } => match agree(SPOKEN, &versions) {
                Some(version) => vec![
                    Event::Welcome { version },
                    Event::Snapshot(self.work.snapshot()),
                ],
                None => vec![Event::Refused {
                    versions: SPOKEN.to_vec(),
                }],
            },
            Request::Snapshot => vec![Event::Snapshot(self.work.snapshot())],
            Request::Watch(sight) => {
                self.sight.send_replace(*sight);
                let left = self.left();
                self.patch(left).into_iter().collect()
            }
            Request::ReadAgain => {
                self.again.send_modify(|n| *n += 1);
                Vec::new()
            }
        }
    }

    /// The host read `changes`: those of what the window still shows that
    /// change the work, as the next patch, or nothing when none do. What was
    /// read for a session or worktree the window has moved away from while
    /// it was read is dropped.
    pub fn record(&mut self, changes: impl IntoIterator<Item = DetailsChange>) -> Option<Event> {
        let sight = self.sight.borrow().clone();
        let shown: Vec<_> = changes
            .into_iter()
            .filter(|c| in_sight(c, &sight))
            .collect();
        self.patch(shown)
    }

    /// `changes` made to the work, those that change it as the next patch.
    fn patch(&mut self, changes: Vec<DetailsChange>) -> Option<Event> {
        let changes: Vec<Change> = changes
            .into_iter()
            .filter(|c| self.work.details.apply(c.clone()))
            .map(Change::Details)
            .collect();
        if changes.is_empty() {
            return None;
        }
        self.work.seq = self.work.seq.next();
        Some(Event::Patch(WorkPatch {
            seq: self.work.seq,
            changes,
        }))
    }

    /// What the host holds of sessions and worktrees the window no longer
    /// shows, to drop.
    fn left(&self) -> Vec<DetailsChange> {
        let sight = self.sight.borrow();
        let sessions = self
            .work
            .details
            .sessions
            .keys()
            .cloned()
            .map(|session| DetailsChange::SessionLeft { session });
        let repositories = self
            .work
            .details
            .repositories
            .keys()
            .cloned()
            .map(|worktree| DetailsChange::RepositoryLeft { worktree });
        sessions
            .chain(repositories)
            .filter(|c| !in_sight(c, &sight))
            .collect()
    }
}

/// Whether `change` is of a session or worktree `sight` shows; one of
/// neither always is.
fn in_sight(change: &DetailsChange, sight: &Sight) -> bool {
    match change {
        DetailsChange::Conversation { session, .. }
        | DetailsChange::Context { session, .. }
        | DetailsChange::Links { session, .. }
        | DetailsChange::Thumbs { session, .. }
        | DetailsChange::Notes { session, .. }
        | DetailsChange::Processes { session, .. }
        | DetailsChange::SessionLeft { session } => {
            sight.session.as_ref().is_some_and(|s| s.id == *session)
        }
        DetailsChange::Repository { worktree, .. } | DetailsChange::RepositoryLeft { worktree } => {
            sight
                .repository
                .as_ref()
                .is_some_and(|r| r.worktree == *worktree)
        }
        DetailsChange::Usage { .. } | DetailsChange::System { .. } => true,
    }
}

/// How a window reaches the work host. Sending never waits: what the host
/// says, in answer or of its own accord, arrives on the receiver the link
/// was made with.
pub trait HostLink {
    fn send(&self, request: Request);
}

/// A host run in the window's own process.
pub struct Loopback {
    host: Arc<Mutex<Host>>,
    events: mpsc::UnboundedSender<Event>,
}

impl Loopback {
    pub fn new(host: Host) -> (Self, mpsc::UnboundedReceiver<Event>) {
        let (events, received) = mpsc::unbounded_channel();
        let link = Self {
            host: Arc::new(Mutex::new(host)),
            events,
        };
        (link, received)
    }

    /// Start reading, in the background, what the window shows: each fact
    /// the host keeps current while it is in sight.
    pub fn watch(&self) {
        crate::watch::start(crate::watch::Watched::new(
            self.host.clone(),
            self.events.clone(),
        ));
    }
}

impl HostLink for Loopback {
    fn send(&self, request: Request) {
        let answer = self
            .host
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .handle(request);
        for event in answer {
            // A window that stopped listening has closed; nothing is owed it.
            let _ = self.events.send(event);
        }
    }
}

/// A host run without a window: `requests` in turn, the work they leave and
/// every event they draw, for tests to read.
pub fn scenario(work: Work, requests: impl IntoIterator<Item = Request>) -> (Work, Vec<Event>) {
    let mut host = Host::new(work);
    let events = requests
        .into_iter()
        .flat_map(|request| host.handle(request))
        .collect();
    (host.work, events)
}

#[cfg(test)]
mod tests {
    use adroit_model::Session;
    use adroit_model::links::Links;
    use adroit_model::repository::{Listing, Spot};
    use adroit_protocol::{
        Event, RepositoryRead, RepositorySight, Request, SPOKEN, Seq, Version, WorkSnapshot,
    };

    use super::*;

    fn hello(versions: &[u32]) -> Request {
        Request::Hello {
            versions: versions.iter().copied().map(Version).collect(),
        }
    }

    #[test]
    fn the_host_answers_hello_by_the_versions_both_speak() {
        let work = Work {
            seq: Seq(4),
            ..Default::default()
        };
        let cases = [
            (
                hello(&[1]),
                vec![
                    Event::Welcome {
                        version: Version(1),
                    },
                    Event::Snapshot(WorkSnapshot {
                        seq: Seq(4),
                        ..Default::default()
                    }),
                ],
            ),
            (
                hello(&[99]),
                vec![Event::Refused {
                    versions: SPOKEN.to_vec(),
                }],
            ),
            (
                hello(&[]),
                vec![Event::Refused {
                    versions: SPOKEN.to_vec(),
                }],
            ),
        ];
        for (request, want) in cases {
            let (_, events) = scenario(work.clone(), [request.clone()]);
            assert_eq!(events, want, "{request:?}");
        }
    }

    #[test]
    fn a_window_that_lost_track_is_sent_the_whole_work() {
        let (work, events) = scenario(
            Work {
                seq: Seq(9),
                ..Default::default()
            },
            [Request::Snapshot],
        );
        assert_eq!(
            events,
            vec![Event::Snapshot(WorkSnapshot {
                seq: Seq(9),
                ..Default::default()
            })]
        );
        assert_eq!(
            work,
            Work {
                seq: Seq(9),
                ..Default::default()
            }
        );
    }

    #[test]
    fn the_loopback_hands_the_answers_to_its_receiver_in_order() {
        let (link, mut events) = Loopback::new(Host::default());
        link.send(hello(&[1]));
        link.send(Request::Snapshot);
        let mut got = Vec::new();
        while let Ok(event) = events.try_recv() {
            got.push(event);
        }
        assert_eq!(
            got,
            vec![
                Event::Welcome {
                    version: Version(1)
                },
                Event::Snapshot(WorkSnapshot::default()),
                Event::Snapshot(WorkSnapshot::default()),
            ]
        );
    }

    #[test]
    fn a_closed_window_does_not_stop_the_host() {
        let (link, events) = Loopback::new(Host::default());
        drop(events);
        link.send(Request::Snapshot);
    }

    fn watch(session: Option<&str>, worktree: Option<&str>) -> Request {
        Request::Watch(Box::new(Sight {
            session: session.map(|id| Session {
                id: id.into(),
                ..Default::default()
            }),
            repository: worktree.map(|w| RepositorySight {
                worktree: w.into(),
                spot: Spot::Log,
            }),
            ..Default::default()
        }))
    }

    fn links(session: &str) -> DetailsChange {
        DetailsChange::Links {
            session: session.into(),
            links: Links::default(),
        }
    }

    fn log(worktree: &str) -> DetailsChange {
        DetailsChange::Repository {
            worktree: worktree.into(),
            read: RepositoryRead {
                spot: Spot::Log,
                listing: Ok(Listing::Log(Vec::new())),
                revision: None,
            },
        }
    }

    fn changes(event: Option<Event>) -> Vec<DetailsChange> {
        match event {
            Some(Event::Patch(patch)) => patch
                .changes
                .into_iter()
                .map(|Change::Details(c)| c)
                .collect(),
            _ => Vec::new(),
        }
    }

    #[test]
    fn what_the_host_reads_reaches_the_window_once_and_only_while_shown() {
        let mut host = Host::default();
        host.handle(watch(Some("a"), Some("/w")));
        // (read, what the window is sent)
        let cases = [
            (vec![links("a")], vec![links("a")]),
            (vec![links("a")], vec![]),
            (vec![links("b"), log("/v")], vec![]),
            (vec![log("/w"), log("/w")], vec![log("/w")]),
        ];
        for (read, want) in cases {
            assert_eq!(changes(host.record(read.clone())), want, "{read:?}");
        }
        assert_eq!(host.work().snapshot().seq, Seq(2));
    }

    #[test]
    fn moving_away_drops_what_was_read_for_what_is_no_longer_shown() {
        let mut host = Host::default();
        host.handle(watch(Some("a"), Some("/w")));
        host.record([links("a"), log("/w")]);
        let events = host.handle(watch(Some("a"), Some("/v")));
        assert_eq!(
            changes(events.into_iter().next()),
            vec![DetailsChange::RepositoryLeft {
                worktree: "/w".into()
            }]
        );
        let events = host.handle(watch(None, None));
        assert_eq!(
            changes(events.into_iter().next()),
            vec![DetailsChange::SessionLeft {
                session: "a".into()
            }]
        );
        assert_eq!(host.work().details(), &DetailsWork::default());
        assert!(host.handle(watch(None, None)).is_empty());
    }

    #[test]
    fn the_watchers_are_told_what_the_window_shows_and_when_to_read_again() {
        let mut host = Host::default();
        let sight = host.sight();
        let again = host.again();
        host.handle(watch(Some("a"), None));
        assert_eq!(
            sight.borrow().session.as_ref().map(|s| s.id.as_str()),
            Some("a")
        );
        assert!(!again.has_changed().unwrap());
        assert!(host.handle(Request::ReadAgain).is_empty());
        assert!(again.has_changed().unwrap());
    }
}
