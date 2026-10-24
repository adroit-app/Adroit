//! What the work host keeps read while windows show it, and the sessions
//! it lists whatever they show: one watcher per fact, each reading again on
//! its own beat or as soon as what it reads for comes into any window's
//! sight, reading once what several windows show, and recording what it
//! read as changes of the work.

use std::collections::BTreeMap;
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::Duration;

use adroit_model::{Session, TerminalSession};
use adroit_protocol::{Change, Event, Sight};
use tokio::sync::watch;

use crate::work::{Host, Seen, Sent, WindowId};

mod commands;
mod config;
mod context;
mod conversation;
mod dialog;
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

/// What tells a window an event; it never waits.
pub type Outbox = Arc<dyn Fn(WindowId, Event) + Send + Sync>;

/// What a watcher reaches the host through.
#[derive(Clone)]
pub struct Watched {
    host: Arc<Mutex<Host>>,
    events: Outbox,
    seen: watch::Receiver<Vec<Seen>>,
    again: watch::Receiver<u64>,
    relist: watch::Receiver<u64>,
    commands: watch::Receiver<u64>,
    inputs: watch::Receiver<BTreeMap<u128, u64>>,
}

impl Watched {
    pub fn new(host: Arc<Mutex<Host>>, events: Outbox) -> Self {
        let (seen, again, relist, commands, inputs) = {
            let host = lock(&host);
            (
                host.seen(),
                host.again(),
                host.relist(),
                host.commands_wanted(),
                host.inputs(),
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
        }
    }

    /// What every window shows now, with the session each shows.
    fn seen(&self) -> Vec<Seen> {
        self.seen.borrow().clone()
    }

    /// What every window shows now.
    fn sights(&self) -> Vec<Sight> {
        self.seen.borrow().iter().map(|s| s.sight.clone()).collect()
    }

    /// When the user last typed into each session, by its terminal.
    fn inputs(&self) -> BTreeMap<u128, u64> {
        self.inputs.borrow().clone()
    }

    pub(crate) fn host(&self) -> MutexGuard<'_, Host> {
        lock(&self.host)
    }

    /// Tell `window` `event`, which is not a change of the work.
    pub(crate) fn tell(&self, window: WindowId, event: Event) {
        (self.events)(window, event);
    }

    /// Send each window what is for it, while the host is still held.
    fn send(&self, sent: Vec<Sent>) {
        for (window, event) in sent {
            (self.events)(window, event);
        }
    }

    /// Place `terminal`, which was opened, in every window's tree.
    pub(crate) fn terminal_opened(&self, terminal: TerminalSession) {
        let mut host = self.host();
        let sent = host.terminal_opened(terminal);
        self.send(sent);
    }

    /// Leave the terminal `id`, which ended, out of every window's tree.
    pub(crate) fn terminal_closed(&self, id: u128) {
        let mut host = self.host();
        let sent = host.terminal_closed(id);
        self.send(sent);
    }

    /// List the sessions again now.
    pub(crate) fn relist(&self) {
        self.host().ask_relist();
    }

    /// Read again now what the windows show.
    pub(crate) fn read_again(&self) {
        self.host().ask_again();
    }

    /// Tell each window what changed of what it shows of what was read.
    pub(crate) fn record<C: Into<Change>>(&self, changes: impl IntoIterator<Item = C>) {
        // Told while the host is held, so patches go out in the order they
        // were made, whichever watcher made them.
        let mut host = self.host();
        let sent = host.record(changes);
        self.send(sent);
    }

    /// As [`Watched::record`], for the windows whose sight `to` takes.
    fn record_to<C: Into<Change>>(
        &self,
        changes: impl IntoIterator<Item = C>,
        to: impl Fn(&Sight) -> bool,
    ) {
        let mut host = self.host();
        let sent = host.record_to(changes, to);
        self.send(sent);
    }

    /// Tell `window` what was read for what it alone asked.
    fn record_for<C: Into<Change>>(&self, window: WindowId, changes: impl IntoIterator<Item = C>) {
        let mut host = self.host();
        let sent = host.record_for(window, changes);
        self.send(sent.into_iter().collect());
    }

    /// Until what `part` takes of what the windows show changes.
    async fn sight_changes<T: PartialEq>(
        seen: &mut watch::Receiver<Vec<Seen>>,
        part: &impl Fn(&[Seen]) -> T,
    ) {
        let was = part(&seen.borrow());
        loop {
            if seen.changed().await.is_err() {
                // The host is gone; only the beat is left.
                std::future::pending::<()>().await;
            }
            if part(&seen.borrow_and_update()) != was {
                return;
            }
        }
    }

    /// Until `raised` is raised.
    async fn raised(raised: &mut watch::Receiver<u64>) {
        if raised.changed().await.is_err() {
            std::future::pending::<()>().await;
        }
    }

    /// Wait `every`, or less once what `part` takes of what the windows
    /// show changes or a window asks for everything read again.
    async fn wait<T: PartialEq>(&mut self, every: Duration, part: impl Fn(&[Seen]) -> T) {
        tokio::select! {
            _ = tokio::time::sleep(every) => {}
            _ = Self::sight_changes(&mut self.seen, &part) => {}
            _ = Self::raised(&mut self.again) => {}
        }
    }

    /// Wait `every`, or less once what `part` takes of what the windows
    /// show changes or a window asks for the sessions listed again; whether
    /// one asked.
    async fn wait_listing<T: PartialEq>(
        &mut self,
        every: Duration,
        part: impl Fn(&[Seen]) -> T,
    ) -> bool {
        tokio::select! {
            _ = tokio::time::sleep(every) => false,
            _ = Self::sight_changes(&mut self.seen, &part) => false,
            _ = Self::raised(&mut self.relist) => true,
        }
    }

    /// Wait until what `part` takes of what the windows show changes, or a
    /// window asks for the commands `/` offers again.
    async fn wait_commands<T: PartialEq>(&mut self, part: impl Fn(&[Seen]) -> T) {
        tokio::select! {
            _ = Self::sight_changes(&mut self.seen, &part) => {}
            _ = Self::raised(&mut self.commands) => {}
        }
    }
}

/// Each distinct value `part` takes of `sights`, in the order first seen.
fn distinct<T: PartialEq>(sights: &[Sight], part: impl Fn(&Sight) -> Option<T>) -> Vec<T> {
    let mut found = Vec::new();
    for value in sights.iter().filter_map(part) {
        if !found.contains(&value) {
            found.push(value);
        }
    }
    found
}

/// Each session a window whose sight `wants` takes shows, once, with
/// whether one of the windows showing it is on its overview.
fn shown_sessions(seen: &[Seen], wants: impl Fn(&Sight) -> bool) -> Vec<(Session, bool)> {
    let mut found: Vec<(Session, bool)> = Vec::new();
    for s in seen.iter().filter(|s| wants(&s.sight)) {
        let Some(session) = &s.session else {
            continue;
        };
        match found.iter_mut().find(|(f, _)| f.id == session.id) {
            Some((_, overview)) => *overview |= s.sight.overview,
            None => found.push((session.clone(), s.sight.overview)),
        }
    }
    found
}

/// The context to ask `agent` in `connection` through: a shown session's, as
/// it was started, when one runs there; else that of a profile the settings
/// in force run there; none when no profile does.
async fn asked_in(
    seen: &[Seen],
    agent: adroit_model::Agent,
    connection: &adroit_model::profile::ConnectionId,
) -> Option<adroit_agents::AgentContext> {
    let shown = seen
        .iter()
        .filter_map(|s| s.session.as_ref())
        .filter(|s| s.agent == agent && s.connection == *connection);
    for session in shown {
        if let Ok(context) = crate::programs::of_session(session).await {
            return Some(context);
        }
    }
    crate::programs::connections(&crate::settings::current())
        .into_iter()
        .find(|c| c.agent() == agent && c.connection == *connection)
}

fn lock(host: &Mutex<Host>) -> MutexGuard<'_, Host> {
    host.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
}

/// The watcher of the configuration, which the windows work by from the
/// first, run in the background for as long as the process lives.
pub fn start_config(watched: Watched) {
    tokio::spawn(config::watch(watched));
}

/// Every watcher, run in the background for as long as the process lives.
pub fn start(watched: Watched) {
    let (jobs, histories, redraws, quits) = {
        let mut host = watched.host();
        (
            host.take_jobs(),
            host.take_histories(),
            host.take_redraws(),
            host.take_quits(),
        )
    };
    if let Some(jobs) = jobs {
        tokio::spawn(crate::jobs::run(watched.clone(), jobs));
    }
    if let Some(histories) = histories {
        tokio::spawn(history::watch(watched.clone(), histories));
    }
    if let Some(redraws) = redraws {
        tokio::spawn(dialog::watch(watched.clone(), redraws));
    }
    if let Some(quits) = quits {
        tokio::spawn(crate::jobs::quit(quits));
    }
    tokio::spawn(sessions::watch(watched.clone()));
    tokio::spawn(marks::watch(watched.clone()));
    tokio::spawn(past::watch(watched.clone()));
    tokio::spawn(offers::watch(watched.clone()));
    tokio::spawn(commands::watch(watched.clone()));
    tokio::spawn(place::watch(watched.clone()));
    tokio::spawn(entries::watch(watched.clone()));
    tokio::spawn(repository::watch(watched.clone()));
    tokio::spawn(conversation::watch(watched.clone()));
    tokio::spawn(context::watch(watched.clone()));
    tokio::spawn(usage::watch(watched.clone()));
    tokio::spawn(processes::watch(watched.clone()));
    tokio::spawn(system::watch(watched));
}
