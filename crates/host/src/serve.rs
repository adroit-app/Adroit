//! The work host's side of a connection to its socket: it says hello, and
//! then answers what it is asked. A window that says hello in a version both
//! speak is sent the whole work and, from then on, every change to it in the
//! order the host made them; what is asked and waited for is answered to the
//! one that asked alone.

use std::future::Future;
use std::pin::Pin;
use std::sync::{Arc, Mutex, MutexGuard};

use adroit_protocol::{Answer, Ask, Event, Hello, Request, decode, encode};
use serde::Deserialize;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::UnixStream;
use tokio::sync::{broadcast, mpsc};

use crate::watch::Watched;
use crate::work::Host;

/// How many events a window may fall behind before it misses some, and asks
/// for the whole work again.
const BEHIND: usize = 4096;

/// What answers an ask.
pub type Asks = Arc<dyn Fn(Ask) -> Pin<Box<dyn Future<Output = Answer> + Send>> + Send + Sync>;

/// The host every connection reaches, the events every window is sent, and
/// what answers the asks.
#[derive(Clone)]
pub struct Served {
    host: Arc<Mutex<Host>>,
    events: broadcast::Sender<Event>,
    asks: Asks,
}

/// What a connection asks that is not of the work protocol: its shape is the
/// same in every version, so any window can replace any work host.
#[derive(Deserialize)]
#[serde(tag = "op", rename_all = "snake_case")]
enum Op {
    Stop,
}

impl Served {
    /// `host` served, its asks answered by what `asks` makes of the way to
    /// the host and the windows.
    pub fn new(host: Host, asks: impl FnOnce(Watched) -> Asks) -> Self {
        let host = Arc::new(Mutex::new(host));
        let events = broadcast::channel(BEHIND).0;
        let asks = asks(Watched::new(host.clone(), outbox(&events)));
        Self { host, events, asks }
    }

    /// The way to the host and the windows, for what keeps the work read.
    pub fn watched(&self) -> Watched {
        Watched::new(self.host.clone(), outbox(&self.events))
    }

    fn lock(&self) -> MutexGuard<'_, Host> {
        self.host
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    /// Answer `stream` until it hangs up, or until it asks the host to stop,
    /// which `stop` is then told.
    pub(crate) async fn connection(
        &self,
        stream: UnixStream,
        hello: &Hello,
        stop: mpsc::Sender<()>,
    ) {
        let (read, mut write) = stream.into_split();
        let (out, mut outgoing) = mpsc::unbounded_channel::<String>();
        // One who asks without waiting for hello may have hung up already;
        // what it asked is read all the same.
        let _ = out.send(encode(hello));
        let writer = tokio::spawn(async move {
            while let Some(line) = outgoing.recv().await {
                if write.write_all(line.as_bytes()).await.is_err() {
                    break;
                }
            }
        });
        let mut following = None;
        let mut lines = BufReader::new(read).lines();
        while let Ok(Some(line)) = lines.next_line().await {
            if let Ok(Op::Stop) = serde_json::from_str(&line) {
                let _ = out.send("{\"stopping\":true}\n".into());
                let _ = stop.send(()).await;
                break;
            }
            let Ok(request) = decode::<Request>(&line) else {
                continue;
            };
            match request {
                Request::Hello { .. } => {
                    let host = &mut *self.lock();
                    let answer = host.handle(request);
                    let welcomed = answer.iter().any(|e| matches!(e, Event::Welcome { .. }));
                    // Taken while the host is held, so the window is sent
                    // each change after the snapshot, and none before it.
                    let events = (welcomed && following.is_none()).then(|| self.events.subscribe());
                    for event in answer {
                        let _ = out.send(encode(&event));
                    }
                    if let Some(events) = events {
                        following = Some(tokio::spawn(follow(events, out.clone())));
                    }
                }
                Request::Ask { asked, ask } => {
                    let (asks, out) = (self.asks.clone(), out.clone());
                    tokio::spawn(async move {
                        let answer = asks(*ask).await;
                        let _ = out.send(encode(&Event::Answer { asked, answer }));
                    });
                }
                request => {
                    let host = &mut *self.lock();
                    for event in host.handle(request) {
                        match event {
                            // Every window is told a change, in order.
                            Event::Patch(_) => {
                                let _ = self.events.send(event);
                            }
                            event => {
                                let _ = out.send(encode(&event));
                            }
                        }
                    }
                }
            }
        }
        if let Some(following) = following {
            following.abort();
        }
        drop(out);
        let _ = writer.await;
    }
}

/// What tells every window an event, in the order it is told.
fn outbox(events: &broadcast::Sender<Event>) -> crate::watch::Outbox {
    let events = events.clone();
    Arc::new(move |event| {
        // With no window to tell, nothing is owed.
        let _ = events.send(event);
    })
}

/// Send the window each event every window is told. One that fell behind
/// misses some, finds a patch past a gap, and asks for the whole work again.
async fn follow(mut events: broadcast::Receiver<Event>, out: mpsc::UnboundedSender<String>) {
    loop {
        match events.recv().await {
            Ok(event) => {
                if out.send(encode(&event)).is_err() {
                    return;
                }
            }
            Err(broadcast::error::RecvError::Lagged(_)) => {}
            Err(broadcast::error::RecvError::Closed) => return,
        }
    }
}
