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
use tokio::io::{AsyncWriteExt, BufReader};
use tokio::net::UnixStream;
use tokio::sync::{mpsc, watch};

use crate::watch::Watched;
use crate::work::{Host, WindowId};

/// What answers an ask.
pub type Asks = Arc<dyn Fn(Ask) -> Pin<Box<dyn Future<Output = Answer> + Send>> + Send + Sync>;

/// Where each window that said hello is sent its events, by its number.
type Windows = Arc<Mutex<HashMap<WindowId, Out>>>;

#[derive(Clone)]
struct Out {
    queue: mpsc::Sender<String>,
    failed: watch::Sender<bool>,
}

impl Out {
    fn send(&self, line: String) -> Result<(), ()> {
        if line.len() > adroit_protocol::client::MAX_FRAME || self.queue.try_send(line).is_err() {
            let _ = self.failed.send(true);
            return Err(());
        }
        Ok(())
    }
}

/// The host every connection reaches, the windows it sends events to, and
/// what answers the asks.
#[derive(Clone)]
pub struct Served {
    host: Arc<Mutex<Host>>,
    windows: Windows,
    next: Arc<AtomicU64>,
    asks: Asks,
    mutations: crate::mutation::Service,
    environments: Arc<tokio::sync::Mutex<HashMap<String, Served>>>,
    environment: Option<Arc<crate::environment::Environment>>,
}

/// What a connection asks that is not of the work protocol: its shape is the
/// same in every version, so any window can replace any work host.
#[derive(Deserialize)]
#[serde(tag = "op", rename_all = "snake_case")]
enum Op {
    Stop,
    Environment { id: String },
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
            mutations: crate::mutation::Service::default(),
            environments: Arc::default(),
            environment: crate::environment::current(),
        }
    }

    async fn environment(&self, id: &str) -> Result<Served, String> {
        let mut environments = self.environments.lock().await;
        if let Some(served) = environments.get(id) {
            return Ok(served.clone());
        }
        if environments.len() >= 256 {
            return Err("Too many open working environments".into());
        }
        let context =
            crate::environment::Environment::at(id, &adroit_model::paths::instance_state())?;
        let served = crate::environment::run(Some(context), async {
            crate::work_host::environment_served()
        })
        .await;
        environments.insert(id.into(), served.clone());
        Ok(served)
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

    fn windows(&self) -> MutexGuard<'_, HashMap<WindowId, Out>> {
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
        let mut active = self.clone();
        let window = WindowId(self.next.fetch_add(1, Ordering::Relaxed));
        let (read, mut write) = stream.into_split();
        let (queue, mut outgoing) = mpsc::channel::<String>(64);
        let (failed, mut failure) = watch::channel(false);
        let out = Out { queue, failed };
        // One who asks without waiting for hello may have hung up already;
        // what it asked is read all the same.
        let mut hello = hello.clone();
        if hello.endpoint.is_some() {
            hello.mutation_epoch = Some(self.mutations.epoch());
        }
        let remote = hello.endpoint.is_some();
        let mut greeted = false;
        let _ = out.send(encode(&hello));
        let mut writer = crate::environment::spawn(async move {
            while let Some(line) = outgoing.recv().await {
                if !matches!(
                    tokio::time::timeout(
                        std::time::Duration::from_secs(5),
                        write.write_all(line.as_bytes())
                    )
                    .await,
                    Ok(Ok(()))
                ) {
                    break;
                }
            }
        });
        // Dropped once the connection hangs up, which ends what waits.
        let (hung_up, _) = watch::channel(());
        let mut lines = BufReader::new(read);
        let pending = Arc::new(tokio::sync::Semaphore::new(32));
        loop {
            let line = tokio::select! {
                line = adroit_protocol::client::read_line(&mut lines) => match line {
                    Ok(Some(line)) => line,
                    _ => break,
                },
                _ = failure.changed() => break,
            };
            if let Ok(op) = serde_json::from_str(&line) {
                match op {
                    Op::Stop => {
                        let _ = out.send("{\"stopping\":true}\n".into());
                        let _ = stop.send(()).await;
                        break;
                    }
                    Op::Environment { id } => {
                        if greeted || active.environment.is_some() {
                            break;
                        }
                        let selected = self.environment(&id).await;
                        match selected {
                            Ok(next) => {
                                if crate::environment::select(
                                    next.environment.clone().expect("scoped service"),
                                )
                                .is_err()
                                {
                                    break;
                                }
                                let epoch = next.mutations.epoch();
                                active = next;
                                let _ = out.send(format!(
                                    "{}\n",
                                    serde_json::json!({"environment": id, "epoch": epoch})
                                ));
                            }
                            Err(error) => {
                                let _ =
                                    out.send(format!("{}\n", serde_json::json!({"error": error})));
                                break;
                            }
                        }
                        continue;
                    }
                }
            }
            let Ok(request) = decode::<Request>(&line) else {
                continue;
            };
            match request {
                Request::Ask { asked, ask } => {
                    let Ok(permit) = pending.clone().try_acquire_owned() else {
                        break;
                    };
                    if remote && !greeted {
                        let _ = out.send(encode(&Event::Answer {
                            asked,
                            answer: Answer::Failed {
                                text: "greet the work host before requesting work".into(),
                            },
                        }));
                        continue;
                    }
                    match *ask {
                        Ask::MutationEpoch => {
                            let _ = out.send(encode(&Event::Answer {
                                asked,
                                answer: Answer::MutationEpoch {
                                    epoch: active.mutations.epoch(),
                                },
                            }));
                            continue;
                        }
                        Ask::MutationStatus { stamp } => {
                            let _ = out.send(encode(&Event::Answer {
                                asked,
                                answer: Answer::Mutation {
                                    status: active.mutations.query(&stamp),
                                },
                            }));
                            continue;
                        }
                        Ask::Apply { stamp, request } => {
                            let (service, served, out) =
                                (active.mutations.clone(), active.clone(), out.clone());
                            crate::environment::spawn(async move {
                                let _permit = permit;
                                let status = service
                                    .apply(stamp, *request, move |request| async move {
                                        match request {
                                            Request::Ask { ask, .. } => (served.asks)(*ask).await,
                                            request => {
                                                crate::jobs::apply(
                                                    &served.watched(),
                                                    window,
                                                    request,
                                                )
                                                .await
                                            }
                                        }
                                    })
                                    .await;
                                let _ = out.send(encode(&Event::Answer {
                                    asked,
                                    answer: Answer::Mutation { status },
                                }));
                            });
                            continue;
                        }
                        ask if remote && ask.mutates() => {
                            let _ = out.send(encode(&Event::Answer {
                                asked,
                                answer: Answer::Failed {
                                    text: "remote changes require an idempotent mutation request"
                                        .into(),
                                },
                            }));
                            continue;
                        }
                        ask => {
                            let (asks, out) = (active.asks.clone(), out.clone());
                            let waits = ask.waits();
                            let mut gone = hung_up.subscribe();
                            crate::environment::spawn(async move {
                                let _permit = permit;
                                let answering = asks(ask);
                                let answer = match waits {
                                    true => {
                                        tokio::select! { answer = answering => answer, _ = gone.changed() => return }
                                    }
                                    false => answering.await,
                                };
                                let _ = out.send(encode(&Event::Answer { asked, answer }));
                            });
                        }
                    }
                }
                request => {
                    if remote && request.mutates() {
                        let _ = out.send(encode(&Event::Told(adroit_protocol::Told::Failure {
                            text: "remote changes require an idempotent mutation request".into(),
                        })));
                        continue;
                    }
                    // Sent while the host is held, so each window is sent
                    // the changes in the order they were made, and one
                    // welcomed is sent each change after its snapshot and
                    // none before it.
                    let host = &mut *active.lock();
                    let sent = host.handle(window, request);
                    let welcomed = sent
                        .iter()
                        .any(|(to, e)| *to == window && matches!(e, Event::Welcome { .. }));
                    let mut windows = active.windows();
                    for (to, event) in sent {
                        if to == window {
                            let _ = out.send(encode(&event));
                        } else if let Some(other) = windows.get(&to) {
                            let _ = other.send(encode(&event));
                        }
                    }
                    if welcomed {
                        greeted = true;
                        windows.insert(window, out.clone());
                    }
                }
            }
        }
        drop(hung_up);
        {
            let mut host = active.lock();
            active.windows().remove(&window);
            host.leave(window);
        }
        drop(out);
        if tokio::time::timeout(std::time::Duration::from_secs(5), &mut writer)
            .await
            .is_err()
        {
            writer.abort();
            let _ = writer.await;
        }
    }
}

fn lock_windows(windows: &Windows) -> MutexGuard<'_, HashMap<WindowId, Out>> {
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
