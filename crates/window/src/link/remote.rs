//! The remote route never starts or replaces a host on either machine.

mod changes;

use super::{Events, Link, Sending, Shown, Waiting, lock};
use adroit_connect::journal::{Journal, PromptContext, Record};
use adroit_connect::work::Work;
use adroit_connect::{Remote, Tunnel};
use adroit_protocol::client::Connection;
use adroit_protocol::{Answer, Ask, Event, RemoteInfo, Request, SPOKEN, Told};
use changes::Command;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;
use tokio::sync::{Notify, mpsc};
use tokio::time::Instant;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Phase {
    Connecting,
    Connected,
    Reconnecting,
    Error,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Status {
    pub host: String,
    pub phase: Phase,
    pub last_work_ms: Option<u64>,
    pub problem: Option<String>,
    pub retryable: bool,
    pub writable: bool,
    pub pending: Arc<Vec<Record>>,
    pub drafts: Arc<Vec<Record>>,
}

#[cfg(test)]
impl Status {
    /// How a link connected to `host` stands, with nothing pending.
    pub(crate) fn connected(host: &str) -> Self {
        Self {
            host: host.into(),
            phase: Phase::Connected,
            last_work_ms: None,
            problem: None,
            retryable: true,
            writable: true,
            pending: Arc::default(),
            drafts: Arc::default(),
        }
    }
}

struct Shared {
    ready: AtomicBool,
    /// Open only between a snapshot and the loss of its connection, so keys
    /// typed before a disconnect are not delivered after a reconnect.
    input: adroit_term::InputPermission,
    status: std::sync::Mutex<Status>,
    retry: Notify,
    told: mpsc::Sender<Event>,
    journal: std::sync::Mutex<Option<Arc<Journal>>>,
    work: std::sync::Mutex<Option<Arc<Work>>>,
    busy: AtomicBool,
    storage_failed: AtomicBool,
}

impl Shared {
    fn new(host: String, told: mpsc::Sender<Event>, retryable: bool) -> Arc<Self> {
        Arc::new(Self {
            ready: AtomicBool::new(false),
            input: adroit_term::InputPermission::closed(),
            status: std::sync::Mutex::new(Status {
                host,
                phase: Phase::Connecting,
                last_work_ms: None,
                problem: None,
                retryable,
                writable: false,
                pending: Arc::default(),
                drafts: Arc::default(),
            }),
            retry: Notify::new(),
            told,
            journal: std::sync::Mutex::new(None),
            work: std::sync::Mutex::new(None),
            busy: AtomicBool::new(false),
            storage_failed: AtomicBool::new(false),
        })
    }
}

#[derive(Clone)]
pub(super) struct Sender {
    queue: mpsc::Sender<Command>,
    shared: Arc<Shared>,
    /// The work machine as the link reaches it, none when it could not be
    /// made.
    remote: Option<Remote>,
}

impl Sender {
    pub fn status(&self) -> Status {
        lock(&self.shared.status).clone()
    }
    pub fn retry(&self) {
        self.shared.retry.notify_one();
    }
    /// Reach the work machine at `address` from now on, at once. The link
    /// stays the same one, with what it keeps for the work machine.
    pub fn move_to(&self, address: &str) {
        if let Some(remote) = &self.remote {
            remote.moved_to(address);
            self.retry();
        }
    }
    pub fn send(&self, request: Request, waiting: &Waiting) {
        self.send_prompt(request, None, waiting);
    }
    pub fn send_prompt(&self, request: Request, prompt: Option<PromptContext>, waiting: &Waiting) {
        if matches!(request, Request::Hello { .. }) {
            return;
        }
        if matches!(
            request,
            Request::TerminalOpened { .. } | Request::TerminalClosed { .. }
        ) {
            return;
        }
        let mutation = request.mutates();
        let offline = !self.shared.ready.load(Ordering::Acquire);
        let queued_state = matches!(
            request,
            Request::Watch(_)
                | Request::Typed { .. }
                | Request::Snapshot
                | Request::Relist
                | Request::ReadAgain
        );
        let unsupported = matches!(&request, Request::Ask { ask, .. } if matches!(ask.as_ref(), Ask::Apply { .. } | Ask::RestartTerminalHost | Ask::OpenLogin { .. } | Ask::KeepImageFile { .. }));
        let reason = if unsupported {
            Some("This operation is unavailable from a remote window")
        } else if changes::needs_prompt(&request) && prompt.is_none() {
            Some("Remote prompt sends require retained draft context; the request was not sent")
        } else if mutation
            && (offline
                || lock(&self.shared.work).is_none()
                || self.shared.storage_failed.load(Ordering::Acquire))
        {
            Some("Remote changes are unavailable; the request was not sent")
        } else if mutation
            && !self.status().pending.is_empty()
            && !changes::consumes_draft(&request)
        {
            Some("Resolve the pending remote outcome before initiating another change")
        } else if offline && !queued_state {
            Some("The remote host is disconnected; the request was not sent")
        } else {
            None
        };
        if let Some(text) = reason {
            self.refuse(request, waiting, text);
            return;
        }
        if let Err(error) = self.queue.try_send(Command::Work { request, prompt }) {
            let Command::Work { request, .. } = error.into_inner() else {
                unreachable!()
            };
            self.refuse(
                request,
                waiting,
                "The remote request queue is full or closed; the request was not sent",
            );
        }
    }
    pub fn query(&self, stamp: adroit_protocol::MutationStamp) {
        let _ = self.queue.try_send(Command::Query(stamp));
    }
    pub async fn resolve(
        &self,
        stamp: adroit_protocol::MutationStamp,
        checked: bool,
    ) -> Result<(), String> {
        changes::resolve(&self.queue, stamp, checked).await
    }
    pub async fn draft(
        &self,
        stamp: adroit_protocol::MutationStamp,
        forget: bool,
        checked: bool,
    ) -> Result<(), String> {
        changes::draft(&self.queue, stamp, forget, checked).await
    }
    fn refuse(&self, request: Request, waiting: &Waiting, text: &str) {
        if let Request::Ask { asked, .. } = request
            && let Some(answer) = lock(waiting).remove(&asked)
        {
            let _ = answer.send(Answer::Failed { text: text.into() });
        }
        let _ = self
            .shared
            .told
            .try_send(Event::Told(Told::Failure { text: text.into() }));
    }
}

/// Where a remote link keeps the changes it sent and has not seen through,
/// under the instance's folder `state`: the window's own environment where
/// it always has, and each environment shown beside it apart, so two remote
/// links never claim one journal.
pub(crate) fn journal_root(state: &Path, shown: Option<&adroit_model::EnvironmentId>) -> PathBuf {
    match shown {
        None => state.join("remote-journal"),
        Some(environment) => state
            .join("environments")
            .join(environment.to_string())
            .join("remote-journal"),
    }
}

pub(super) fn open(
    remote: Result<Remote, String>,
    scope: Option<String>,
    journal: PathBuf,
) -> (Link, Events, std::sync::mpsc::Receiver<()>) {
    let (queue, requests) = mpsc::channel(64);
    let (told, events) = Events::remote();
    let (greeted, first) = std::sync::mpsc::channel();
    let host = remote
        .as_ref()
        .map(|r| r.name())
        .unwrap_or_else(|_| "remote".into());
    let shared = Shared::new(host, told, remote.is_ok());
    let daemon = remote
        .as_ref()
        .map(|r| r.terminal())
        .unwrap_or_else(|_| adroit_term::Daemon::forwarded(std::path::PathBuf::new()))
        .with_input(shared.input.clone());
    let waiting = Waiting::default();
    let link = Link {
        requests: Sending::Remote(Sender {
            queue,
            shared: shared.clone(),
            remote: remote.as_ref().ok().cloned(),
        }),
        asked: Arc::default(),
        waiting: waiting.clone(),
        daemon,
    };
    match remote {
        Err(text) => {
            lock(&shared.status).phase = Phase::Error;
            lock(&shared.status).problem = Some(text.clone());
            let _ = shared.told.try_send(Event::Told(Told::Failure { text }));
        }
        Ok(remote) => {
            let failure = shared.clone();
            if let Err(error) = std::thread::Builder::new()
                .name("adroit remote host link".into())
                .spawn(move || {
                    let runtime = tokio::runtime::Builder::new_current_thread()
                        .enable_all()
                        .build();
                    match runtime {
                        Ok(runtime) => runtime.block_on(
                            Keep {
                                remote,
                                scope,
                                journal,
                                requests,
                                shared,
                                waiting,
                                greeted: Some(greeted),
                                expected: None,
                                tunnel: None,
                            }
                            .run(),
                        ),
                        Err(error) => {
                            let mut status = lock(&shared.status);
                            status.phase = Phase::Error;
                            status.retryable = false;
                            status.problem =
                                Some(format!("cannot start the remote runtime: {error}"));
                        }
                    }
                })
            {
                lock(&failure.status).phase = Phase::Error;
                lock(&failure.status).retryable = false;
                lock(&failure.status).problem =
                    Some(format!("cannot start the remote connection: {error}"));
            }
        }
    }
    (link, events, first)
}

struct Keep {
    remote: Remote,
    scope: Option<String>,
    /// Where the changes sent and not seen through are kept.
    journal: PathBuf,
    requests: mpsc::Receiver<Command>,
    shared: Arc<Shared>,
    waiting: Waiting,
    greeted: Option<std::sync::mpsc::Sender<()>>,
    expected: Option<RemoteInfo>,
    /// Held only so the forwarded sockets stay open while connected.
    tunnel: Option<Tunnel>,
}

impl Keep {
    fn offline(&self, command: Command, shown: &mut Shown) {
        let Command::Work { request, .. } = command else {
            self.shared.control(command);
            return;
        };
        shown.note(&request);
        if let Request::Ask { asked, .. } = request
            && let Some(waiting) = lock(&self.waiting).remove(&asked)
        {
            let _ = waiting.send(Answer::Failed {
                text: "The remote host disconnected before the request was sent".into(),
            });
        }
    }
    fn phase(&self, phase: Phase, problem: Option<String>) {
        let mut status = lock(&self.shared.status);
        status.phase = phase;
        if problem.is_some()
            || phase != Phase::Connected
            || !self.shared.storage_failed.load(Ordering::Acquire)
        {
            status.problem = problem;
        }
    }
    async fn connect(&mut self) -> Result<Connection, String> {
        let tunnel = self.remote.connect(self.expected.as_ref()).await?;
        let connection = Connection::open_in(&self.remote.work_socket(), self.scope.as_deref())
            .await
            .map_err(|e| e.to_string())?;
        if connection.hello.endpoint.as_ref() != Some(&tunnel.info().work)
            || connection.hello.terminal.as_ref() != Some(&tunnel.info().terminal)
        {
            return Err("remote host identity changed before opening the window".into());
        }
        self.expected = Some(tunnel.info().clone());
        let journal = lock(&self.shared.journal).clone();
        let journal = match journal {
            Some(journal) => Ok(journal),
            None => {
                let root = &self.journal;
                root.parent()
                    .map_or(Ok(()), std::fs::create_dir_all)
                    .map_err(|e| e.to_string())
                    .and_then(|_| Journal::open(root, &tunnel.info().instance))
                    .map(Arc::new)
            }
        };
        match journal.and_then(|journal| {
            tunnel
                .work(journal.clone(), self.scope.clone())
                .map(|work| (journal, Arc::new(work)))
        }) {
            Ok((journal, work)) => {
                *lock(&self.shared.journal) = Some(journal);
                *lock(&self.shared.work) = Some(work);
                self.shared.storage_failed.store(false, Ordering::Release);
            }
            Err(text) => {
                self.shared.storage_failed.store(true, Ordering::Release);
                lock(&self.shared.status).problem =
                    Some(format!("Remote changes are unavailable: {text}"));
            }
        }
        self.shared.refresh_changes();
        self.tunnel = Some(tunnel);
        Ok(connection)
    }
    async fn run(mut self) {
        let mut shown = Shown::default();
        let mut failures = 0u32;
        loop {
            self.shared.ready.store(false, Ordering::Release);
            *lock(&self.shared.work) = None;
            self.shared.refresh_changes();
            self.phase(
                if self.expected.is_some() {
                    Phase::Reconnecting
                } else {
                    Phase::Connecting
                },
                None,
            );
            let result = self.connect().await;
            match result {
                Ok(connection) => {
                    let (closed, received) = self.serve(connection, &mut shown).await;
                    failures = if received { 0 } else { failures + 1 };
                    if closed {
                        return;
                    }
                }
                Err(text) => {
                    failures += 1;
                    self.phase(Phase::Reconnecting, Some(text.clone()));
                    let _ = self
                        .shared
                        .told
                        .try_send(Event::Told(Told::Failure { text }));
                }
            }
            self.shared.ready.store(false, Ordering::Release);
            self.tunnel = None;
            let fatal = lock(&self.shared.status).problem.as_ref().is_some_and(|p| {
                p.contains("identity or generation changed")
                    || p.contains("protocols are incompatible")
                    || p.contains("refused this client's key")
                    || p.contains("is not the paired work machine")
            });
            if fatal || failures >= 5 {
                let problem = lock(&self.shared.status).problem.clone();
                self.phase(Phase::Error, problem);
                loop {
                    tokio::select! {
                        _ = self.shared.retry.notified() => { failures = 0; break; },
                        request = self.requests.recv() => match request { Some(request) => self.offline(request, &mut shown), None => return },
                    }
                }
            } else {
                let delay =
                    tokio::time::sleep(Duration::from_millis(500 * (1u64 << failures.min(4))));
                tokio::pin!(delay);
                loop {
                    tokio::select! {
                        _ = &mut delay => break,
                        _ = self.shared.retry.notified() => break,
                        request = self.requests.recv() => match request { Some(request) => self.offline(request, &mut shown), None => return },
                    }
                }
            }
        }
    }
    async fn serve(&mut self, connection: Connection, shown: &mut Shown) -> (bool, bool) {
        let Connection {
            mut events,
            mut requests,
            ..
        } = connection;
        for request in std::iter::once(&Request::Hello {
            versions: SPOKEN.to_vec(),
        })
        .chain(shown.again())
        {
            if requests.send(request).await.is_err() {
                return (false, false);
            }
        }
        let mut pending = HashMap::new();
        let mut received = false;
        let mut tick = tokio::time::interval(Duration::from_millis(250));
        let mut greeting = Some(Instant::now() + Duration::from_secs(5));
        let closed = loop {
            tokio::select! {
                event = events.next() => match event {
                    Ok(Some(Event::Answer { asked, answer })) => {
                        pending.remove(&asked);
                        if let Some(waiting) = lock(&self.waiting).remove(&asked) { let _ = waiting.send(answer); }
                    },
                    Ok(Some(event)) => {
                        if matches!(event, Event::Snapshot(_)) {
                            received = true;
                            greeting = None;
                            self.shared.ready.store(true, Ordering::Release);
                            self.shared.input.set(true);
                            self.phase(Phase::Connected, None);
                            self.shared.refresh_changes();
                            self.shared.recover_pending();
                            if let Some(greeted) = self.greeted.take() { let _ = greeted.send(()); }
                        }
                        if matches!(event, Event::Snapshot(_) | Event::Patch(_)) { lock(&self.shared.status).last_work_ms = Some(adroit_model::time::now_ms()); }
                        if let Err(error) = self.shared.told.try_send(event) {
                            match error {
                                mpsc::error::TrySendError::Full(_) => break false,
                                mpsc::error::TrySendError::Closed(_) => break true,
                            }
                        }
                    },
                    _ => break false,
                },
                request = self.requests.recv() => match request {
                    Some(command) => {
                        let Command::Work { request, prompt } = command else { self.shared.control(command); continue; };
                        if request.mutates() { self.shared.change(request, prompt, self.waiting.clone()); continue; }
                        shown.note(&request);
                        if let Request::Ask { asked, .. } = &request {
                            if pending.len() >= 32 {
                                if let Some(waiting) = lock(&self.waiting).remove(asked) { let _ = waiting.send(Answer::Failed { text: "Too many remote reads are pending".into() }); }
                                continue;
                            }
                            pending.insert(*asked, Instant::now() + Duration::from_secs(30));
                        }
                        if requests.send(&request).await.is_err() { break false; }
                    },
                    None => break true,
                },
                _ = tick.tick() => {
                    if greeting.is_some_and(|deadline| Instant::now() >= deadline) { break false; }
                    pending.retain(|asked, deadline| {
                        if Instant::now() < *deadline { return true; }
                        if let Some(waiting) = lock(&self.waiting).remove(asked) { let _ = waiting.send(Answer::Failed { text: "The remote read timed out".into() }); }
                        false
                    });
                },
                _ = self.shared.retry.notified() => break false,
            }
        };
        self.shared.input.set(false);
        self.shared.ready.store(false, Ordering::Release);
        *lock(&self.shared.work) = None;
        self.shared.refresh_changes();
        self.phase(Phase::Reconnecting, None);
        for asked in pending.keys() {
            if let Some(waiting) = lock(&self.waiting).remove(asked) {
                let _ = waiting.send(Answer::Failed {
                    text: "The remote host disconnected before answering".into(),
                });
            }
        }
        (closed, received)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn a_remote_snapshot_enables_reads_and_console_input_and_a_disconnect_closes_them() {
        use adroit_protocol::{Hello, decode, encode};
        use tokio::io::{AsyncWriteExt, BufReader};
        let key = tempfile::tempdir().unwrap();
        let remote = Remote::with_key(
            &adroit_connect::Target::Native {
                address: "work.local:47130".into(),
                server: "ab".repeat(32),
            },
            &key.path().join("client.key"),
        )
        .unwrap();
        let listener = tokio::net::UnixListener::bind(remote.work_socket()).unwrap();
        let (queue, requests) = mpsc::channel(8);
        let (told, _events) = mpsc::channel(8);
        let shared = Shared::new("work".into(), told, true);
        assert_eq!(shared.input.ticket(), None, "closed before connecting");
        let sender = Sender {
            queue,
            shared: shared.clone(),
            remote: None,
        };
        let waiting = Waiting::default();
        let mut keep = Keep {
            remote: remote.clone(),
            scope: None,
            journal: key.path().join("journal"),
            requests,
            shared: shared.clone(),
            waiting: waiting.clone(),
            greeted: None,
            expected: None,
            tunnel: None,
        };
        let server = tokio::spawn(async move {
            let (mut stream, _) = listener.accept().await.unwrap();
            stream
                .write_all(
                    encode(&Hello {
                        speaks: SPOKEN.to_vec(),
                        build: "test".into(),
                        endpoint: None,
                        terminal: None,
                        mutation_epoch: None,
                    })
                    .as_bytes(),
                )
                .await
                .unwrap();
            let (read, mut write) = stream.into_split();
            let mut read = BufReader::new(read);
            let request = adroit_protocol::client::read_line(&mut read)
                .await
                .unwrap()
                .unwrap();
            assert!(matches!(
                decode::<Request>(&request).unwrap(),
                Request::Hello { .. }
            ));
            write
                .write_all(encode(&Event::Welcome { version: SPOKEN[0] }).as_bytes())
                .await
                .unwrap();
            write
                .write_all(encode(&Event::from(adroit_protocol::testing::snapshot())).as_bytes())
                .await
                .unwrap();
            let request = adroit_protocol::client::read_line(&mut read)
                .await
                .unwrap()
                .unwrap();
            let Request::Ask { asked, ask } = decode::<Request>(&request).unwrap() else {
                panic!("expected a read");
            };
            assert_eq!(*ask, Ask::TerminalHost);
            write
                .write_all(
                    encode(&Event::Answer {
                        asked,
                        answer: Answer::Done,
                    })
                    .as_bytes(),
                )
                .await
                .unwrap();
        });
        let connection = Connection::open_in(&remote.work_socket(), None)
            .await
            .unwrap();
        let served =
            tokio::spawn(async move { keep.serve(connection, &mut Shown::default()).await });
        tokio::time::timeout(Duration::from_secs(2), async {
            while !shared.ready.load(Ordering::Acquire) {
                tokio::task::yield_now().await;
            }
        })
        .await
        .unwrap();
        let typed = shared.input.ticket();
        assert!(typed.is_some(), "console input opens with the snapshot");
        let (reply, answered) = tokio::sync::oneshot::channel();
        lock(&waiting).insert(1, reply);
        sender.send(
            Request::Ask {
                asked: 1,
                ask: Box::new(Ask::RestartTerminalHost),
            },
            &waiting,
        );
        assert!(matches!(answered.await.unwrap(), Answer::Failed { .. }));
        let (reply, answered) = tokio::sync::oneshot::channel();
        lock(&waiting).insert(2, reply);
        sender.send(
            Request::Ask {
                asked: 2,
                ask: Box::new(Ask::TerminalHost),
            },
            &waiting,
        );
        assert_eq!(
            tokio::time::timeout(Duration::from_secs(2), answered)
                .await
                .unwrap()
                .unwrap(),
            Answer::Done
        );
        server.await.unwrap();
        assert_eq!(
            tokio::time::timeout(Duration::from_secs(2), served)
                .await
                .unwrap()
                .unwrap(),
            (false, true)
        );
        assert!(!shared.ready.load(Ordering::Acquire));
        assert_eq!(shared.input.ticket(), None, "closed by the disconnect");
        assert!(
            !shared.input.allows(typed.unwrap()),
            "keys queued before the disconnect never reach the program"
        );
        let (reply, answered) = tokio::sync::oneshot::channel();
        lock(&waiting).insert(3, reply);
        sender.send(
            Request::Ask {
                asked: 3,
                ask: Box::new(Ask::TerminalHost),
            },
            &waiting,
        );
        assert!(matches!(answered.await.unwrap(), Answer::Failed { .. }));
        assert!(lock(&waiting).is_empty());
    }

    #[test]
    fn each_environment_shown_beside_the_window_s_own_keeps_a_journal_of_its_own() {
        let state = Path::new("/state");
        let own = journal_root(state, None);
        let a = journal_root(state, Some(&adroit_model::EnvironmentId::new("a")));
        let b = journal_root(state, Some(&adroit_model::EnvironmentId::new("b")));
        assert_eq!(own, Path::new("/state/remote-journal"));
        assert_ne!(a, b);
        assert!(!a.starts_with(&own) && !b.starts_with(&own));
    }

    #[tokio::test]
    async fn invalid_execution_configuration_never_falls_back_to_starting_a_local_host() {
        let dir = tempfile::tempdir().unwrap();
        let config = dir.path().join("execution.toml");
        std::fs::write(
            &config,
            "backend = 'native'\naddress = '-invalid'\nserver = 'unknown'\n",
        )
        .unwrap();
        let marker = dir.path().join("local-host-started");
        let route = super::super::Route::unbound(
            adroit_connect::Target::load(&config),
            super::super::Toward::Local {
                host: dir.path().join("local.sock"),
                terminal: dir.path().join("felis.sock"),
                work: "default".into(),
            },
        );
        let (link, mut events, first) = Link::selected(
            route,
            |socket, _, _| super::super::To {
                socket,
                program: "/usr/bin/touch".into(),
                args: vec![marker.to_string_lossy().into_owned()],
                build: "test".into(),
            },
            dir.path().join("journal"),
        );
        assert!(first.recv_timeout(Duration::from_secs(1)).is_err());
        assert_eq!(link.remote_status().unwrap().phase, Phase::Error);
        let answer = tokio::time::timeout(Duration::from_secs(1), link.ask(Ask::TerminalHost))
            .await
            .unwrap();
        assert!(matches!(answer, Answer::Failed { .. }));
        assert!(!marker.exists());
        assert!(matches!(
            events.recv().await,
            Some(Event::Told(Told::Failure { .. }))
        ));
        assert!(matches!(
            link.daemon()
                .start(
                    Path::new("/usr/bin/touch"),
                    &[marker.to_string_lossy().into_owned()]
                )
                .await,
            Err(adroit_term::Error::Forwarded(_))
        ));
        assert!(!marker.exists());
    }
}
