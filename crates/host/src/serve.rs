//! The work host's side of a connection to its socket: it says hello, and
//! then answers what it is asked. Each connection is a window of its own: a
//! window that says hello in a version both speak is sent the whole work as
//! it shows it and, from then on, every change to that in the order the
//! host made them; what one window asks, and what is asked and waited for,
//! is answered to the one that asked alone.

use std::collections::HashMap;
use std::future::Future;
use std::pin::Pin;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};

use adroit_protocol::{Answer, Ask, Event, Hello, Request, decode, encode};
use serde::Deserialize;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::UnixStream;
use tokio::sync::{mpsc, watch};

use crate::watch::Watched;
use crate::work::{Host, WindowId};

/// What answers an ask.
pub type Asks = Arc<dyn Fn(Ask) -> Pin<Box<dyn Future<Output = Answer> + Send>> + Send + Sync>;

/// Where each window that said hello is sent its events, by its number.
type Windows = Arc<Mutex<HashMap<WindowId, mpsc::UnboundedSender<String>>>>;

/// The host every connection reaches, the windows it sends events to, and
/// what answers the asks.
#[derive(Clone)]
pub struct Served {
    host: Arc<Mutex<Host>>,
    windows: Windows,
    next: Arc<AtomicU64>,
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
        let windows = Windows::default();
        let asks = asks(Watched::new(host.clone(), outbox(&windows)));
        Self {
            host,
            windows,
            next: Arc::new(AtomicU64::new(1)),
            asks,
        }
    }

    /// The way to the host and the windows, for what keeps the work read.
    pub fn watched(&self) -> Watched {
        Watched::new(self.host.clone(), outbox(&self.windows))
    }

    fn lock(&self) -> MutexGuard<'_, Host> {
        self.host
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    fn windows(&self) -> MutexGuard<'_, HashMap<WindowId, mpsc::UnboundedSender<String>>> {
        lock_windows(&self.windows)
    }

    /// Answer `stream` until it hangs up, or until it asks the host to stop,
    /// which `stop` is then told.
    pub(crate) async fn connection(
        &self,
        stream: UnixStream,
        hello: &Hello,
        stop: mpsc::Sender<()>,
    ) {
        let window = WindowId(self.next.fetch_add(1, Ordering::Relaxed));
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
        // Dropped once the connection hangs up, which ends what waits.
        let (hung_up, _) = watch::channel(());
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
                Request::Ask { asked, ask } => {
                    let (asks, out) = (self.asks.clone(), out.clone());
                    let waits = ask.waits();
                    let mut gone = hung_up.subscribe();
                    tokio::spawn(async move {
                        let answering = asks(*ask);
                        let answer = match waits {
                            true => tokio::select! {
                                answer = answering => answer,
                                _ = gone.changed() => return,
                            },
                            false => answering.await,
                        };
                        let _ = out.send(encode(&Event::Answer { asked, answer }));
                    });
                }
                request => {
                    // Sent while the host is held, so each window is sent
                    // the changes in the order they were made, and one
                    // welcomed is sent each change after its snapshot and
                    // none before it.
                    let host = &mut *self.lock();
                    let sent = host.handle(window, request);
                    let welcomed = sent
                        .iter()
                        .any(|(to, e)| *to == window && matches!(e, Event::Welcome { .. }));
                    let mut windows = self.windows();
                    for (to, event) in sent {
                        if to == window {
                            let _ = out.send(encode(&event));
                        } else if let Some(other) = windows.get(&to) {
                            let _ = other.send(encode(&event));
                        }
                    }
                    if welcomed {
                        windows.insert(window, out.clone());
                    }
                }
            }
        }
        drop(hung_up);
        {
            let mut host = self.lock();
            self.windows().remove(&window);
            host.leave(window);
        }
        drop(out);
        let _ = writer.await;
    }
}

fn lock_windows(
    windows: &Windows,
) -> MutexGuard<'_, HashMap<WindowId, mpsc::UnboundedSender<String>>> {
    windows
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

/// What tells a window that said hello an event, in the order it is told.
fn outbox(windows: &Windows) -> crate::watch::Outbox {
    let windows = windows.clone();
    Arc::new(move |window, event| {
        // A window gone, or not yet welcomed, is owed nothing.
        if let Some(out) = lock_windows(&windows).get(&window) {
            let _ = out.send(encode(&event));
        }
    })
}
