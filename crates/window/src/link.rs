//! How the window reaches the work host: through its socket, starting one
//! where none answers and replacing one of another build, and connecting
//! again whenever it goes. Each connection opens with hello and with what
//! the window shows, so a host replaced meanwhile sends the whole work and
//! keeps read what the window shows. The link runs on a thread of its own,
//! so the window can wait for the first snapshot before it is drawn.

use std::collections::{HashMap, HashSet};
use std::future::Future;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use adroit_protocol::client::{Connection, ask_stop, hello};
use adroit_protocol::{Answer, Ask, Event, Request, SPOKEN, Told, WorkHost, judge};
use tokio::sync::{mpsc, oneshot};

/// What an ask in flight is answered when the host goes before answering.
const GONE: &str = "The work host went away before it answered; try again";

/// How long the link waits before trying the host again.
const AGAIN: Duration = Duration::from_millis(500);

/// How long a work host just started is given to say hello.
const STARTING: Duration = Duration::from_secs(5);

/// The work host the window works with: where it listens, and how it is
/// started.
#[derive(Debug, Clone)]
pub(crate) struct To {
    pub(crate) socket: PathBuf,
    pub(crate) program: PathBuf,
    pub(crate) args: Vec<String>,
    /// This build, which a host kept has to be of.
    pub(crate) build: String,
}

/// The asks sent and not yet answered, by their numbers.
type Waiting = Arc<Mutex<HashMap<u64, oneshot::Sender<Answer>>>>;

/// The window's end of the link.
pub(crate) struct Link {
    requests: mpsc::UnboundedSender<Request>,
    asked: AtomicU64,
    waiting: Waiting,
}

impl Link {
    /// Keep a link to the work host `to` on a thread of its own: the link,
    /// what the host tells the window, and what is told once the window has
    /// first been sent the work.
    pub(crate) fn open(
        to: To,
    ) -> (
        Link,
        mpsc::UnboundedReceiver<Event>,
        std::sync::mpsc::Receiver<()>,
    ) {
        let (requests, asked) = mpsc::unbounded_channel();
        let (told, events) = mpsc::unbounded_channel();
        let (greeted, first) = std::sync::mpsc::channel();
        let waiting = Waiting::default();
        let keeping = Keep {
            to,
            asked,
            told,
            waiting: waiting.clone(),
            greeted: Some(greeted),
        };
        let started = std::thread::Builder::new()
            .name("adroit work host link".into())
            .spawn(move || {
                let Ok(rt) = tokio::runtime::Builder::new_current_thread()
                    .enable_all()
                    .build()
                else {
                    return;
                };
                rt.block_on(keeping.run());
            });
        if let Err(e) = started {
            eprintln!("adroit: cannot reach the work host: {e}");
        }
        let link = Link {
            requests,
            asked: AtomicU64::new(0),
            waiting,
        };
        (link, events, first)
    }

    /// Send `request`, now or once the host answers again.
    pub(crate) fn send(&self, request: Request) {
        // A link whose thread is gone has nothing to send through.
        let _ = self.requests.send(request);
    }

    /// Ask `ask`, now or once the host answers again, and what came of it.
    pub(crate) fn ask(&self, ask: Ask) -> impl Future<Output = Answer> + use<> {
        let asked = self.asked.fetch_add(1, Ordering::Relaxed) + 1;
        let (answer, answered) = oneshot::channel();
        lock(&self.waiting).insert(asked, answer);
        self.send(Request::Ask {
            asked,
            ask: Box::new(ask),
        });
        async move {
            answered
                .await
                .unwrap_or_else(|_| Answer::Failed { text: GONE.into() })
        }
    }
}

fn lock<T>(m: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
}

/// What the link says again to a host it connects to after the first: what
/// the window shows and when the user last typed, as it last said them.
#[derive(Debug, Default)]
struct Shown {
    watch: Option<Request>,
    typed: Option<Request>,
}

impl Shown {
    fn note(&mut self, request: &Request) {
        match request {
            Request::Watch(_) => self.watch = Some(request.clone()),
            Request::Typed { .. } => self.typed = Some(request.clone()),
            _ => {}
        }
    }

    fn again(&self) -> impl Iterator<Item = &Request> {
        self.watch.iter().chain(self.typed.iter())
    }
}

/// The link's own thread.
struct Keep {
    to: To,
    asked: mpsc::UnboundedReceiver<Request>,
    told: mpsc::UnboundedSender<Event>,
    waiting: Waiting,
    greeted: Option<std::sync::mpsc::Sender<()>>,
}

impl Keep {
    /// Keep the window linked to the host until the window closes.
    async fn run(mut self) {
        let mut shown = Shown::default();
        loop {
            let connection = match self.connect().await {
                Ok(connection) => connection,
                Err(text) => {
                    self.tell(Told::Failure { text });
                    tokio::time::sleep(AGAIN).await;
                    continue;
                }
            };
            match self.serve(connection, &mut shown).await {
                Served::Closed => return,
                Served::Gone => tokio::time::sleep(AGAIN).await,
            }
        }
    }

    /// A connection to a work host of this build, started where none
    /// answers and replaced where one of another build does.
    async fn connect(&self) -> Result<Connection, String> {
        ensure(&self.to)
            .await
            .map_err(|e| format!("cannot start the work host: {e}"))?;
        Connection::open(&self.to.socket)
            .await
            .map_err(|e| format!("cannot reach the work host: {e}"))
    }

    /// Pass what the window asks to the host and what it tells to the
    /// window, until either goes.
    async fn serve(&mut self, connection: Connection, shown: &mut Shown) -> Served {
        let Connection {
            mut events,
            mut requests,
            ..
        } = connection;
        let mut in_flight = HashSet::new();
        let greeting = Request::Hello {
            versions: SPOKEN.to_vec(),
        };
        for request in std::iter::once(&greeting).chain(shown.again()) {
            if requests.send(request).await.is_err() {
                return Served::Gone;
            }
        }
        let gone = loop {
            tokio::select! {
                event = events.next() => match event {
                    Ok(Some(event)) => self.take(event, &mut in_flight),
                    _ => break Served::Gone,
                },
                request = self.asked.recv() => match request {
                    // The window greets through the link, which greets
                    // every host it connects to.
                    Some(Request::Hello { .. }) => {}
                    Some(request) => {
                        shown.note(&request);
                        if let Request::Ask { asked, .. } = &request {
                            in_flight.insert(*asked);
                        }
                        if requests.send(&request).await.is_err() {
                            break Served::Gone;
                        }
                    }
                    None => break Served::Closed,
                },
            }
        };
        // What was asked of the host gone is not known to be done.
        let mut waiting = lock(&self.waiting);
        for asked in in_flight {
            if let Some(answer) = waiting.remove(&asked) {
                let _ = answer.send(Answer::Failed { text: GONE.into() });
            }
        }
        gone
    }

    /// `event`, told by the host: an answer to whoever asked, anything else
    /// to the window.
    fn take(&mut self, event: Event, in_flight: &mut HashSet<u64>) {
        match event {
            Event::Answer { asked, answer } => {
                in_flight.remove(&asked);
                if let Some(waiting) = lock(&self.waiting).remove(&asked) {
                    let _ = waiting.send(answer);
                }
            }
            event => {
                let snapshot = matches!(event, Event::Snapshot(_));
                // A window that stopped listening has closed.
                let _ = self.told.send(event);
                if snapshot && let Some(greeted) = self.greeted.take() {
                    let _ = greeted.send(());
                }
            }
        }
    }

    fn tell(&self, told: Told) {
        let _ = self.told.send(Event::Told(told));
    }
}

/// How serving a connection ended.
enum Served {
    /// The host went.
    Gone,
    /// The window closed.
    Closed,
}

/// Make sure a work host of this build answers as `to` says: start one where
/// none does, and replace one of another build, or one this build cannot
/// work with, which ends no session.
async fn ensure(to: &To) -> Result<(), String> {
    let said = hello(&to.socket).await;
    match judge(said.as_ref().map(Option::as_ref), &to.build) {
        WorkHost::Keep => return Ok(()),
        WorkHost::Replace => {
            adroit_term::stop_listening(&to.socket, || async { Ok(ask_stop(&to.socket).await?) })
                .await
                .map_err(|e| e.to_string())?;
        }
        WorkHost::Absent => {}
    }
    start(&to.socket, &to.program, &to.args).await
}

/// Start `program` with `args` apart from the window as the work host at
/// `socket`, and wait until it says hello.
async fn start(socket: &Path, program: &Path, args: &[String]) -> Result<(), String> {
    adroit_term::own_dir(socket).map_err(|e| e.to_string())?;
    adroit_term::detached(program, args).map_err(|e| e.to_string())?;
    let until = tokio::time::Instant::now() + STARTING;
    while tokio::time::Instant::now() < until {
        if let Some(Some(_)) = hello(socket).await {
            return Ok(());
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    Err(format!(
        "the work host at {} did not start",
        socket.display()
    ))
}

#[cfg(test)]
mod tests {
    use adroit_protocol::{Hello, Sight, decode, encode};
    use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
    use tokio::net::UnixListener;

    use super::*;

    const BUILD: &str = "0.0.0 (test)";

    /// A folder for a socket, whose path has to be short.
    fn short_dir() -> tempfile::TempDir {
        let dir = tempfile::Builder::new()
            .prefix("adroit-")
            .tempdir_in("/tmp")
            .unwrap();
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(dir.path(), std::fs::Permissions::from_mode(0o700)).unwrap();
        }
        dir
    }

    fn to(socket: &Path) -> To {
        To {
            socket: socket.into(),
            // Never started: a host of this build always answers.
            program: "/usr/bin/false".into(),
            args: Vec::new(),
            build: BUILD.into(),
        }
    }

    /// One connection to a host that says hello as this build, the requests
    /// it is sent read as they come.
    struct Accepted {
        first: Option<String>,
        lines: tokio::io::Lines<BufReader<tokio::net::unix::OwnedReadHalf>>,
        write: tokio::net::unix::OwnedWriteHalf,
    }

    impl Accepted {
        /// The next connection that is not a look at the hello alone.
        async fn from(listener: &UnixListener) -> Self {
            loop {
                let (stream, _) = listener.accept().await.unwrap();
                let (read, mut write) = stream.into_split();
                let hello = Hello {
                    speaks: SPOKEN.to_vec(),
                    build: BUILD.into(),
                };
                if write.write_all(encode(&hello).as_bytes()).await.is_err() {
                    continue;
                }
                let mut lines = BufReader::new(read).lines();
                // A look at the hello hangs up without asking anything.
                let Ok(Ok(Some(first))) =
                    tokio::time::timeout(Duration::from_secs(5), lines.next_line()).await
                else {
                    continue;
                };
                return Self {
                    first: Some(first),
                    lines,
                    write,
                };
            }
        }

        async fn asked(&mut self) -> Request {
            if let Some(first) = self.first.take() {
                return decode(first).unwrap();
            }
            let line = tokio::time::timeout(Duration::from_secs(5), self.lines.next_line())
                .await
                .expect("nothing was asked")
                .unwrap()
                .expect("the window hung up");
            decode(line).unwrap()
        }

        async fn tell(&mut self, event: &Event) {
            self.write
                .write_all(encode(event).as_bytes())
                .await
                .unwrap();
        }
    }

    fn watch(session: &str) -> Request {
        Request::Watch(Box::new(Sight {
            session: Some(session.into()),
            ..Default::default()
        }))
    }

    async fn told(events: &mut mpsc::UnboundedReceiver<Event>) -> Event {
        tokio::time::timeout(Duration::from_secs(5), events.recv())
            .await
            .expect("nothing was told")
            .unwrap()
    }

    #[tokio::test]
    async fn the_window_greets_the_host_and_is_told_it_was_sent_the_work() {
        let dir = short_dir();
        let socket = dir.path().join("h.sock");
        let listener = UnixListener::bind(&socket).unwrap();
        let (link, mut events, first) = Link::open(to(&socket));
        let mut host = Accepted::from(&listener).await;
        assert!(matches!(host.asked().await, Request::Hello { .. }));
        // The window's own greeting is the link's to say.
        link.send(Request::Hello {
            versions: SPOKEN.to_vec(),
        });
        link.send(Request::Snapshot);
        assert_eq!(host.asked().await, Request::Snapshot);
        host.tell(&Event::Snapshot(Box::default())).await;
        assert_eq!(told(&mut events).await, Event::Snapshot(Box::default()));
        first
            .recv_timeout(Duration::from_secs(5))
            .expect("not told of the first snapshot");
    }

    #[tokio::test]
    async fn a_host_replaced_is_greeted_and_told_again_what_the_window_shows() {
        let dir = short_dir();
        let socket = dir.path().join("h.sock");
        let listener = UnixListener::bind(&socket).unwrap();
        let (link, mut events, _) = Link::open(to(&socket));
        let mut host = Accepted::from(&listener).await;
        host.asked().await;
        link.send(watch("a"));
        link.send(watch("b"));
        link.send(Request::ReadAgain);
        for _ in 0..3 {
            host.asked().await;
        }
        drop(host);
        let mut again = Accepted::from(&listener).await;
        assert!(matches!(again.asked().await, Request::Hello { .. }));
        assert_eq!(again.asked().await, watch("b"));
        let snapshot = Event::Snapshot(Box::default());
        again.tell(&snapshot).await;
        assert_eq!(told(&mut events).await, snapshot);
    }

    #[tokio::test]
    async fn an_ask_is_answered_by_its_number_and_one_in_flight_fails_when_the_host_goes() {
        let dir = short_dir();
        let socket = dir.path().join("h.sock");
        let listener = UnixListener::bind(&socket).unwrap();
        let (link, _events, _) = Link::open(to(&socket));
        let mut host = Accepted::from(&listener).await;
        host.asked().await;
        let answered = link.ask(Ask::TerminalHost);
        let Request::Ask { asked, .. } = host.asked().await else {
            panic!("not asked");
        };
        host.tell(&Event::Answer {
            asked,
            answer: Answer::Done,
        })
        .await;
        assert_eq!(answered.await, Answer::Done);
        let unanswered = link.ask(Ask::TerminalHost);
        assert!(matches!(host.asked().await, Request::Ask { .. }));
        drop(host);
        assert_eq!(unanswered.await, Answer::Failed { text: GONE.into() });
    }
}
