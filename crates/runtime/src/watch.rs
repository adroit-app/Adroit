//! What the work host keeps read while a window shows it, and the sessions
//! it lists whatever the window shows: one watcher per fact, each reading
//! again on its own beat or as soon as what it reads for comes into sight,
//! and recording what it read as changes of the work.

use std::collections::BTreeMap;
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::Duration;

use adroit_protocol::{Change, Event, Sight};
use tokio::sync::{mpsc, watch};

use crate::work::{HistoryAsk, Host, Seen};

mod commands;
mod config;
mod context;
mod conversation;
mod entries;
mod history;
mod marks;
mod offers;
mod past;
mod place;
mod processes;
mod repository;
mod sessions;
mod system;
mod usage;

pub use sessions::read_sessions;

/// What a watcher reaches the host through.
#[derive(Clone)]
pub struct Watched {
    host: Arc<Mutex<Host>>,
    events: mpsc::UnboundedSender<Event>,
    seen: watch::Receiver<Seen>,
    again: watch::Receiver<u64>,
    relist: watch::Receiver<u64>,
    commands: watch::Receiver<u64>,
    inputs: watch::Receiver<BTreeMap<u128, u64>>,
    history: watch::Receiver<Option<HistoryAsk>>,
}

impl Watched {
    pub fn new(host: Arc<Mutex<Host>>, events: mpsc::UnboundedSender<Event>) -> Self {
        let (seen, again, relist, commands, inputs, history) = {
            let host = lock(&host);
            (
                host.seen(),
                host.again(),
                host.relist(),
                host.commands_wanted(),
                host.inputs(),
                host.history(),
            )
        };
        Self {
            host,
            events,
            seen,
            again,
            relist,
            commands,
            inputs,
            history,
        }
    }

    /// What the window shows now.
    fn sight(&self) -> Sight {
        self.seen.borrow().sight.clone()
    }

    /// What the window shows now, with the session it shows.
    fn seen(&self) -> Seen {
        self.seen.borrow().clone()
    }

    /// When the user last typed into each session, by its terminal.
    fn inputs(&self) -> BTreeMap<u128, u64> {
        self.inputs.borrow().clone()
    }

    /// The next history the window asks for; none once the host is gone.
    async fn asked_history(&mut self) -> Option<HistoryAsk> {
        self.history.changed().await.ok()?;
        self.history.borrow_and_update().clone()
    }

    fn host(&self) -> MutexGuard<'_, Host> {
        lock(&self.host)
    }

    /// Tell the window `event`, which is not a change of the work.
    pub(crate) fn tell(&self, event: Event) {
        // A window that stopped listening has closed; nothing is owed it.
        let _ = self.events.send(event);
    }

    /// List the sessions again now.
    pub(crate) fn relist(&self) {
        self.host().ask_relist();
    }

    /// Read again now what the window shows.
    pub(crate) fn read_again(&self) {
        self.host().ask_again();
    }

    /// Tell the window what changed of what was read.
    fn record<C: Into<Change>>(&self, changes: impl IntoIterator<Item = C>) {
        let event = self.host().record(changes);
        if let Some(event) = event {
            // A window that stopped listening has closed; nothing is owed it.
            let _ = self.events.send(event);
        }
    }

    /// Wait `every`, or less once what `part` takes of the sight changes or
    /// the window asks for everything read again.
    async fn wait<T: PartialEq>(&mut self, every: Duration, part: impl Fn(&Seen) -> T) {
        let was = part(&self.seen.borrow());
        let changed = async {
            loop {
                if self.seen.changed().await.is_err() {
                    // The host is gone; only the beat is left.
                    std::future::pending::<()>().await;
                }
                if part(&self.seen.borrow_and_update()) != was {
                    return;
                }
            }
        };
        let again = async {
            if self.again.changed().await.is_err() {
                std::future::pending::<()>().await;
            }
        };
        tokio::select! {
            _ = tokio::time::sleep(every) => {}
            _ = changed => {}
            _ = again => {}
        }
    }
}

impl Watched {
    /// Wait `every`, or less once what `part` takes of the sight changes or
    /// the window asks for the sessions listed again; whether it asked.
    async fn wait_listing<T: PartialEq>(
        &mut self,
        every: Duration,
        part: impl Fn(&Seen) -> T,
    ) -> bool {
        let was = part(&self.seen.borrow());
        let changed = async {
            loop {
                if self.seen.changed().await.is_err() {
                    std::future::pending::<()>().await;
                }
                if part(&self.seen.borrow_and_update()) != was {
                    return;
                }
            }
        };
        let relist = async {
            if self.relist.changed().await.is_err() {
                std::future::pending::<()>().await;
            }
        };
        tokio::select! {
            _ = tokio::time::sleep(every) => false,
            _ = changed => false,
            _ = relist => true,
        }
    }
}

impl Watched {
    /// Wait until what `part` takes of the sight changes, or the window asks
    /// for the commands `/` offers again.
    async fn wait_commands<T: PartialEq>(&mut self, part: impl Fn(&Seen) -> T) {
        let was = part(&self.seen.borrow());
        let changed = async {
            loop {
                if self.seen.changed().await.is_err() {
                    std::future::pending::<()>().await;
                }
                if part(&self.seen.borrow_and_update()) != was {
                    return;
                }
            }
        };
        let wanted = async {
            if self.commands.changed().await.is_err() {
                std::future::pending::<()>().await;
            }
        };
        tokio::select! {
            _ = changed => {}
            _ = wanted => {}
        }
    }
}

fn lock(host: &Mutex<Host>) -> MutexGuard<'_, Host> {
    host.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
}

/// The watcher of the configuration, which the window works by from the
/// first, run in the background for as long as the process lives.
pub fn start_config(watched: Watched) {
    tokio::spawn(config::watch(watched));
}

/// Every watcher, run in the background for as long as the process lives.
pub fn start(watched: Watched) {
    if let Some(jobs) = watched.host().take_jobs() {
        tokio::spawn(crate::jobs::run(watched.clone(), jobs));
    }
    tokio::spawn(sessions::watch(watched.clone()));
    tokio::spawn(marks::watch(watched.clone()));
    tokio::spawn(past::watch(watched.clone()));
    tokio::spawn(offers::watch(watched.clone()));
    tokio::spawn(commands::watch(watched.clone()));
    tokio::spawn(place::watch(watched.clone()));
    tokio::spawn(history::watch(watched.clone()));
    tokio::spawn(entries::watch(watched.clone()));
    tokio::spawn(repository::watch(watched.clone()));
    tokio::spawn(conversation::watch(watched.clone()));
    tokio::spawn(context::watch(watched.clone()));
    tokio::spawn(usage::watch(watched.clone()));
    tokio::spawn(processes::watch(watched.clone()));
    tokio::spawn(system::watch(watched));
}
