//! What the work host keeps read while a window shows it, and the sessions
//! it lists whatever the window shows: one watcher per fact, each reading
//! again on its own beat or as soon as what it reads for comes into sight,
//! and recording what it read as changes of the work.

use std::collections::BTreeMap;
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::Duration;

use adroit_protocol::{Change, Event, Sight};
use tokio::sync::{mpsc, watch};

use crate::work::Host;

mod context;
mod conversation;
mod entries;
mod marks;
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
    sight: watch::Receiver<Sight>,
    again: watch::Receiver<u64>,
    relist: watch::Receiver<u64>,
    inputs: watch::Receiver<BTreeMap<u128, u64>>,
}

impl Watched {
    pub fn new(host: Arc<Mutex<Host>>, events: mpsc::UnboundedSender<Event>) -> Self {
        let (sight, again, relist, inputs) = {
            let host = lock(&host);
            (host.sight(), host.again(), host.relist(), host.inputs())
        };
        Self {
            host,
            events,
            sight,
            again,
            relist,
            inputs,
        }
    }

    /// What the window shows now.
    fn sight(&self) -> Sight {
        self.sight.borrow().clone()
    }

    /// When the user last typed into each session, by its terminal.
    fn inputs(&self) -> BTreeMap<u128, u64> {
        self.inputs.borrow().clone()
    }

    fn host(&self) -> MutexGuard<'_, Host> {
        lock(&self.host)
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
    async fn wait<T: PartialEq>(&mut self, every: Duration, part: impl Fn(&Sight) -> T) {
        let was = part(&self.sight.borrow());
        let changed = async {
            loop {
                if self.sight.changed().await.is_err() {
                    // The host is gone; only the beat is left.
                    std::future::pending::<()>().await;
                }
                if part(&self.sight.borrow_and_update()) != was {
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
        part: impl Fn(&Sight) -> T,
    ) -> bool {
        let was = part(&self.sight.borrow());
        let changed = async {
            loop {
                if self.sight.changed().await.is_err() {
                    std::future::pending::<()>().await;
                }
                if part(&self.sight.borrow_and_update()) != was {
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

fn lock(host: &Mutex<Host>) -> MutexGuard<'_, Host> {
    host.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
}

/// Every watcher, run in the background for as long as the process lives.
pub fn start(watched: Watched) {
    tokio::spawn(sessions::watch(watched.clone()));
    tokio::spawn(marks::watch(watched.clone()));
    tokio::spawn(entries::watch(watched.clone()));
    tokio::spawn(repository::watch(watched.clone()));
    tokio::spawn(conversation::watch(watched.clone()));
    tokio::spawn(context::watch(watched.clone()));
    tokio::spawn(usage::watch(watched.clone()));
    tokio::spawn(processes::watch(watched.clone()));
    tokio::spawn(system::watch(watched));
}
