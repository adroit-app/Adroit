//! The work host: the work it holds, how it answers a window's requests,
//! and the link a window reaches it through.

use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};

use adroit_model::{Session, TerminalSession};
use adroit_protocol::{
    Change, DetailsChange, DetailsWork, Event, Request, SPOKEN, Seq, Sight, TreeChange, TreeWork,
    WorkPatch, WorkSnapshot, agree,
};
use tokio::sync::{mpsc, watch};

/// Everything the host holds of the work.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Work {
    seq: Seq,
    details: DetailsWork,
    tree: TreeWork,
}

impl Work {
    /// The work as a window is first shown it.
    pub fn snapshot(&self) -> WorkSnapshot {
        WorkSnapshot {
            seq: self.seq,
            details: self.details.clone(),
            tree: self.tree.clone(),
        }
    }

    pub fn details(&self) -> &DetailsWork {
        &self.details
    }

    pub fn tree(&self) -> &TreeWork {
        &self.tree
    }

    /// The work with `change` made; whether that changed it.
    fn apply(&mut self, change: Change) -> bool {
        match change {
            Change::Details(change) => self.details.apply(change),
            Change::Tree(change) => self.tree.apply(change),
        }
    }
}

/// What a window shows, with the session it shows as the host lists it.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Seen {
    pub sight: Sight,
    pub session: Option<Session>,
}

#[derive(Debug)]
pub struct Host {
    work: Work,
    /// What the window shows, which the host keeps read.
    seen: watch::Sender<Seen>,
    /// Raised each time the window asks for everything read again.
    again: watch::Sender<u64>,
    /// Raised each time the window asks for the sessions listed again.
    relist: watch::Sender<u64>,
    /// When the user last typed into each session the window shows, by its
    /// terminal.
    inputs: watch::Sender<BTreeMap<u128, u64>>,
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
            seen: watch::Sender::new(Seen::default()),
            again: watch::Sender::new(0),
            relist: watch::Sender::new(0),
            inputs: watch::Sender::new(BTreeMap::new()),
        }
    }

    pub fn work(&self) -> &Work {
        &self.work
    }

    /// What the window shows, as it changes.
    pub fn seen(&self) -> watch::Receiver<Seen> {
        self.seen.subscribe()
    }

    /// Each time the window asks for everything read again.
    pub fn again(&self) -> watch::Receiver<u64> {
        self.again.subscribe()
    }

    /// Each time the window asks for the sessions listed again.
    pub fn relist(&self) -> watch::Receiver<u64> {
        self.relist.subscribe()
    }

    /// When the user last typed into each session, as the window last said.
    pub fn inputs(&self) -> watch::Receiver<BTreeMap<u128, u64>> {
        self.inputs.subscribe()
    }

    /// What the host tells the window that asked `request`.
    pub fn handle(&mut self, request: Request) -> Vec<Event> {
        match request {
            Request::Hello { versions } => match agree(SPOKEN, &versions) {
                Some(version) => vec![
                    Event::Welcome { version },
                    Event::Snapshot(Box::new(self.work.snapshot())),
                ],
                None => vec![Event::Refused {
                    versions: SPOKEN.to_vec(),
                }],
            },
            Request::Snapshot => vec![Event::Snapshot(Box::new(self.work.snapshot()))],
            Request::Watch(sight) => {
                let session = self.listed(sight.session.as_deref());
                self.seen.send_replace(Seen {
                    sight: *sight,
                    session,
                });
                let left = self.left();
                self.patch(left).into_iter().collect()
            }
            Request::ReadAgain => {
                self.again.send_modify(|n| *n += 1);
                Vec::new()
            }
            Request::Relist => {
                self.relist.send_modify(|n| *n += 1);
                Vec::new()
            }
            Request::Typed { inputs } => {
                self.inputs.send_if_modified(|was| {
                    let changed = *was != inputs;
                    *was = inputs;
                    changed
                });
                Vec::new()
            }
            Request::TerminalOpened { terminal } => {
                let id = terminal.id;
                self.terminals(|terminals| {
                    if !terminals.iter().any(|t| t.id == id) {
                        terminals.push(terminal);
                    }
                })
            }
            Request::TerminalClosed { id } => self.terminals(|terminals| {
                terminals.retain(|t| t.id != id);
            }),
        }
    }

    /// The session listed by `id`.
    fn listed(&self, id: Option<&str>) -> Option<Session> {
        let id = id?;
        self.work
            .tree
            .sessions()
            .iter()
            .find(|s| s.id == id)
            .cloned()
    }

    /// The terminals listed as `change` leaves them, placed in the tree at
    /// once rather than when the sessions are next listed.
    fn terminals(&mut self, change: impl FnOnce(&mut Vec<TerminalSession>)) -> Vec<Event> {
        let mut listed = self.work.tree.listed.clone().unwrap_or_default();
        change(&mut listed.terminals);
        listed.tree = listed.tree.with_terminals(&listed.terminals);
        let listed = TreeChange::Listed { listed };
        self.patch(vec![listed.into()]).into_iter().collect()
    }

    /// The host read `changes`: those of what the window still shows that
    /// change the work, as the next patch, or nothing when none do. What was
    /// read for a session or worktree the window has moved away from while
    /// it was read is dropped.
    pub fn record<C: Into<Change>>(
        &mut self,
        changes: impl IntoIterator<Item = C>,
    ) -> Option<Event> {
        let sight = self.seen.borrow().sight.clone();
        let shown: Vec<_> = changes
            .into_iter()
            .map(Into::into)
            .filter(|c| in_sight(c, &sight))
            .collect();
        self.patch(shown)
    }

    /// `changes` made to the work, those that change it as the next patch.
    fn patch(&mut self, changes: Vec<Change>) -> Option<Event> {
        let changes: Vec<Change> = changes
            .into_iter()
            .filter(|c| self.work.apply(c.clone()))
            .collect();
        if changes.is_empty() {
            return None;
        }
        self.work.seq = self.work.seq.next();
        if changes
            .iter()
            .any(|c| matches!(c, Change::Tree(TreeChange::Listed { .. })))
        {
            let session = self.listed(self.seen.borrow().sight.session.as_deref());
            self.seen.send_if_modified(|seen| {
                let changed = seen.session != session;
                seen.session = session;
                changed
            });
        }
        Some(Event::Patch(WorkPatch {
            seq: self.work.seq,
            changes,
        }))
    }

    /// What the host holds of sessions and worktrees the window no longer
    /// shows, to drop.
    fn left(&self) -> Vec<Change> {
        let sight = &self.seen.borrow().sight;
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
        let marks = self
            .work
            .tree
            .marks
            .keys()
            .cloned()
            .map(|worktree| TreeChange::MarksLeft { worktree });
        let entries = self
            .work
            .tree
            .entries
            .keys()
            .cloned()
            .map(|dir| TreeChange::EntriesLeft { dir });
        sessions
            .chain(repositories)
            .map(Change::from)
            .chain(marks.chain(entries).map(Change::from))
            .filter(|c| !in_sight(c, sight))
            .collect()
    }
}

/// Whether `change` is of something `sight` shows; one of nothing shown in
/// particular always is.
fn in_sight(change: &Change, sight: &Sight) -> bool {
    match change {
        Change::Details(change) => details_in_sight(change, sight),
        Change::Tree(TreeChange::Listed { .. } | TreeChange::Failing { .. }) => true,
        Change::Tree(TreeChange::Marks { worktree, .. } | TreeChange::MarksLeft { worktree }) => {
            sight.tree.marked.contains(worktree)
        }
        Change::Tree(TreeChange::Entries { dir, .. } | TreeChange::EntriesLeft { dir }) => {
            sight.tree.listed.contains(dir)
        }
    }
}

fn details_in_sight(change: &DetailsChange, sight: &Sight) -> bool {
    match change {
        DetailsChange::Conversation { session, .. }
        | DetailsChange::Context { session, .. }
        | DetailsChange::Links { session, .. }
        | DetailsChange::Thumbs { session, .. }
        | DetailsChange::Notes { session, .. }
        | DetailsChange::Processes { session, .. }
        | DetailsChange::SessionLeft { session } => sight.session.as_ref() == Some(session),
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
                    Event::Snapshot(Box::new(WorkSnapshot {
                        seq: Seq(4),
                        ..Default::default()
                    })),
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
            vec![Event::Snapshot(Box::new(WorkSnapshot {
                seq: Seq(9),
                ..Default::default()
            }))]
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
                Event::Snapshot(Box::default()),
                Event::Snapshot(Box::default()),
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
            session: session.map(Into::into),
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
                .filter_map(|c| match c {
                    Change::Details(c) => Some(c),
                    Change::Tree(_) => None,
                })
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
        let seen = host.seen();
        let again = host.again();
        host.handle(watch(Some("a"), None));
        assert_eq!(seen.borrow().sight.session.as_deref(), Some("a"));
        assert!(!again.has_changed().unwrap());
        assert!(host.handle(Request::ReadAgain).is_empty());
        assert!(again.has_changed().unwrap());
    }

    fn tree_changes(events: Vec<Event>) -> Vec<TreeChange> {
        events
            .into_iter()
            .flat_map(|e| match e {
                Event::Patch(patch) => patch.changes,
                _ => Vec::new(),
            })
            .filter_map(|c| match c {
                Change::Tree(c) => Some(c),
                Change::Details(_) => None,
            })
            .collect()
    }

    fn sees(marked: &[&str], listed: &[&str]) -> Request {
        Request::Watch(Box::new(Sight {
            tree: adroit_protocol::TreeSight {
                marked: marked.iter().map(Into::into).collect(),
                listed: listed.iter().map(Into::into).collect(),
                ..Default::default()
            },
            ..Default::default()
        }))
    }

    fn entries(dir: &str) -> TreeChange {
        TreeChange::Entries {
            dir: dir.into(),
            entries: Vec::new(),
        }
    }

    fn marks(worktree: &str) -> TreeChange {
        TreeChange::Marks {
            worktree: worktree.into(),
            marks: Default::default(),
        }
    }

    #[test]
    fn the_sessions_reach_the_window_whatever_it_shows_and_the_files_only_while_listed() {
        let mut host = Host::default();
        host.handle(sees(&["/w"], &["/w"]));
        let failing = TreeChange::Failing {
            error: Some("no daemon".into()),
        };
        // (read, what the window is sent)
        let cases = [
            (vec![failing.clone()], vec![failing]),
            (
                vec![entries("/w"), marks("/w")],
                vec![entries("/w"), marks("/w")],
            ),
            (vec![entries("/v"), marks("/v")], vec![]),
        ];
        for (read, want) in cases {
            let got = tree_changes(host.record(read.clone()).into_iter().collect());
            assert_eq!(got, want, "{read:?}");
        }
        let left = tree_changes(host.handle(sees(&[], &[])));
        assert_eq!(
            left,
            [
                TreeChange::MarksLeft {
                    worktree: "/w".into()
                },
                TreeChange::EntriesLeft { dir: "/w".into() },
            ]
        );
        assert!(host.work().tree().entries.is_empty());
    }

    fn terminal(id: u128, cwd: &str) -> TerminalSession {
        TerminalSession {
            id,
            name: format!("shell {id}"),
            cwd: cwd.into(),
            ..Default::default()
        }
    }

    #[test]
    fn a_terminal_the_window_opens_or_closes_is_placed_at_once() {
        let mut host = Host::default();
        let session = Session {
            id: "a".into(),
            cwd: "/w".into(),
            ..Default::default()
        };
        let tree = adroit_model::build(
            std::slice::from_ref(&session),
            &[],
            &Default::default(),
            &[],
            None,
        );
        host.record([TreeChange::Listed {
            listed: Box::new(adroit_protocol::Listed {
                sessions: vec![session],
                terminals: Vec::new(),
                tree,
            }),
        }]);
        let opened = host.handle(Request::TerminalOpened {
            terminal: terminal(7, "/w"),
        });
        assert_eq!(tree_changes(opened).len(), 1);
        let ids = |host: &Host| {
            host.work()
                .tree()
                .tree()
                .worktrees()
                .flat_map(|w| w.terminals.iter().map(|t| t.id))
                .collect::<Vec<_>>()
        };
        assert_eq!(ids(&host), [7]);
        assert_eq!(host.work().tree().terminals().len(), 1);
        let again = host.handle(Request::TerminalOpened {
            terminal: terminal(7, "/w"),
        });
        assert!(
            again.is_empty(),
            "a terminal listed already changes nothing"
        );
        host.handle(Request::TerminalClosed { id: 7 });
        assert!(ids(&host).is_empty());
        assert!(host.work().tree().terminals().is_empty());
    }

    #[test]
    fn the_lister_is_told_when_to_list_again_and_what_was_typed() {
        let mut host = Host::default();
        let relist = host.relist();
        let inputs = host.inputs();
        assert!(host.handle(Request::Relist).is_empty());
        assert!(relist.has_changed().unwrap());
        let typed = BTreeMap::from([(7, 1_000)]);
        assert!(
            host.handle(Request::Typed {
                inputs: typed.clone()
            })
            .is_empty()
        );
        assert!(inputs.has_changed().unwrap());
        assert_eq!(*inputs.borrow(), typed);
    }

    #[test]
    fn the_watchers_read_the_session_shown_as_the_host_lists_it() {
        let mut host = Host::default();
        let seen = host.seen();
        let listing = |session_id: &str| TreeChange::Listed {
            listed: Box::new(adroit_protocol::Listed {
                sessions: vec![Session {
                    id: "a".into(),
                    session_id: session_id.into(),
                    ..Default::default()
                }],
                ..Default::default()
            }),
        };
        host.handle(watch(Some("a"), None));
        assert_eq!(seen.borrow().session, None, "not listed yet");
        host.record([listing("first")]);
        let conversation = || seen.borrow().session.as_ref().map(|s| s.session_id.clone());
        assert_eq!(conversation().as_deref(), Some("first"));
        host.record([listing("resumed")]);
        assert_eq!(conversation().as_deref(), Some("resumed"));
        host.handle(watch(None, None));
        assert_eq!(seen.borrow().session, None);
    }
}
