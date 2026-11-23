//! What the window knows of each environment's work host: whether they
//! speak, and the work as that host last described it.

use std::collections::BTreeMap;

use adroit_model::{EnvironmentId, Session};
use adroit_protocol::{Event, Request, SPOKEN, Sight, Told, Version, WorkPatch, WorkSnapshot};

use crate::chrome::status_line;
use crate::content::Tab;
use crate::effect::Effect;
use crate::state::ViewState;
use crate::tree::{self, Place, nav};

/// How the window stands with the work host.
#[derive(Debug, Clone, Default, PartialEq)]
pub enum Link {
    /// The window said hello and waits for the answer.
    #[default]
    Opening,
    /// They work together in `version`.
    Ready { version: Version },
    /// They share no version; the host speaks these.
    Refused { versions: Vec<Version> },
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct Host {
    pub link: Link,
    /// The work as of the last snapshot and the patches after it, or none
    /// before the first snapshot.
    pub work: Option<WorkSnapshot>,
    /// A snapshot was asked for and has not come yet.
    resyncing: bool,
    /// What the host was last asked to keep read.
    pub(crate) watched: Sight,
    /// What the environment is called, once the window is told.
    name: Option<String>,
    /// The environment is on a work machine, not this computer.
    remote: bool,
    /// The failure the host last told, said once until its work comes
    /// again: a link that cannot reach its host tells it at every try.
    failed: Option<String>,
}

impl Host {
    /// What the host was last asked to keep read.
    pub fn watched(&self) -> &Sight {
        &self.watched
    }
}

/// The host of each environment the window shows, held apart: the same
/// session id in two of them is two sessions.
#[derive(Debug, Clone, PartialEq)]
pub struct Hosts {
    /// The window's own environment, the one it was opened in: what it
    /// starts goes there, and its work heads the tree.
    pub environment: EnvironmentId,
    /// Ordered by environment, so whatever lists them lists them alike each
    /// time. Always holds the host of `environment`.
    each: BTreeMap<EnvironmentId, Host>,
}

impl Default for Hosts {
    fn default() -> Self {
        let environment = EnvironmentId::default();
        Self {
            each: BTreeMap::from([(environment.clone(), Host::default())]),
            environment,
        }
    }
}

impl Hosts {
    /// The host of the window's own environment.
    pub fn own(&self) -> &Host {
        &self.each[&self.environment]
    }

    pub(crate) fn own_mut(&mut self) -> &mut Host {
        self.each.entry(self.environment.clone()).or_default()
    }

    pub fn get(&self, environment: &EnvironmentId) -> Option<&Host> {
        self.each.get(environment)
    }

    /// The work of `environment`'s host, none before its first snapshot or
    /// for an environment the window has not heard from.
    pub fn work(&self, environment: &EnvironmentId) -> Option<&WorkSnapshot> {
        self.get(environment).and_then(|h| h.work.as_ref())
    }

    /// The session by `id` in `environment`, with the worktree it runs in.
    pub fn session(&self, environment: &EnvironmentId, id: &str) -> Option<(Place, &Session)> {
        nav::find(tree::listed(self.work(environment)), id)
            .map(|(p, s)| (Place::at(environment, p), s))
    }

    /// The environment of what `tab` shows, the window's own with nothing
    /// open.
    pub fn environment_of<'a>(&'a self, tab: Option<&'a Tab>) -> &'a EnvironmentId {
        tab.map_or(&self.environment, Tab::environment)
    }

    /// Every environment the window shows with its host, its own first and
    /// the others in the order of their ids, so whatever lists them lists
    /// them alike each time.
    pub fn each(&self) -> impl Iterator<Item = (&EnvironmentId, &Host)> {
        let own = (&self.environment, self.own());
        std::iter::once(own).chain(self.each.iter().filter(|(e, _)| **e != self.environment))
    }

    pub(crate) fn each_mut(&mut self) -> impl Iterator<Item = (&EnvironmentId, &mut Host)> {
        self.each.iter_mut()
    }

    /// What `environment` is called: the name the window was told, or else
    /// its id.
    pub fn name(&self, environment: &EnvironmentId) -> String {
        let told = self.get(environment).and_then(|h| h.name.clone());
        match told {
            Some(name) => name,
            None if environment.is_unbound() => "This environment".into(),
            None => environment.to_string(),
        }
    }

    /// The window shows `environment`, called `name`. A host it had not
    /// heard from is greeted; the window's own was greeted when it opened.
    pub(crate) fn add(&mut self, environment: EnvironmentId, name: String) -> Vec<Effect> {
        let new = !self.each.contains_key(&environment);
        self.each.entry(environment.clone()).or_default().name = Some(name);
        if new {
            vec![hello(environment)]
        } else {
            Vec::new()
        }
    }

    /// Whether `environment`'s files are on this computer's disk, for the
    /// window's viewers to read: those of any environment the window was not
    /// told is on a work machine.
    pub fn is_local(&self, environment: &EnvironmentId) -> bool {
        !self.get(environment).is_some_and(|h| h.remote)
    }

    /// `environment`, one the window shows, is on a work machine.
    pub(crate) fn on_work_machine(&mut self, environment: &EnvironmentId) {
        if let Some(host) = self.each.get_mut(environment) {
            host.remote = true;
        }
    }

    /// `request` sent to the host of every environment the window shows.
    pub(crate) fn ask_each(&self, request: Request) -> Vec<Effect> {
        self.each()
            .map(|(environment, _)| Effect::Host {
                environment: environment.clone(),
                request: request.clone(),
            })
            .collect()
    }

    /// The window is opened in `environment`; its host until then, which
    /// has heard nothing yet, becomes that environment's.
    pub(crate) fn open(&mut self, environment: EnvironmentId) {
        let host = self.each.remove(&self.environment).unwrap_or_default();
        self.each.entry(environment.clone()).or_insert(host);
        self.environment = environment;
    }
}

/// The window stops listing `environment`: its host and the windows showing
/// its work go, and the window is told to drop its link. Its sessions run
/// on; the environment the window was started in stays.
pub(crate) fn close(state: &mut ViewState, environment: &EnvironmentId) -> Vec<Effect> {
    if *environment == state.hosts.environment || state.hosts.each.remove(environment).is_none() {
        return Vec::new();
    }
    crate::content::forget_environment(&mut state.content, environment);
    crate::overlay::forget_daemon(state, environment);
    if state
        .tree
        .selected
        .as_ref()
        .is_some_and(|row| row.environment() == environment)
    {
        state.tree.selected = None;
    }
    vec![Effect::EnvironmentClosed(environment.clone())]
}

/// The first request a window sends, to the host of `environment`.
pub(crate) fn hello(environment: EnvironmentId) -> Effect {
    Effect::Host {
        environment,
        request: Request::Hello {
            versions: SPOKEN.to_vec(),
        },
    }
}

/// The host of `environment` told the window `event`.
fn receive(environment: &EnvironmentId, host: &mut Host, event: Event) -> Vec<Effect> {
    match event {
        Event::Welcome { version } if SPOKEN.contains(&version) => {
            host.link = Link::Ready { version };
            host.failed = None;
        }
        Event::Welcome { version } => {
            host.link = Link::Refused {
                versions: vec![version],
            };
        }
        Event::Refused { versions } => host.link = Link::Refused { versions },
        Event::Snapshot(snapshot) => {
            host.work = Some(*snapshot);
            host.resyncing = false;
            host.failed = None;
        }
        Event::Patch(patch) => {
            host.failed = None;
            return patched(environment, host, patch);
        }
        // Told the window as a whole, not its link; an answer goes to
        // what asked.
        Event::Told(_) | Event::Type { .. } | Event::Answer { .. } => {}
    }
    Vec::new()
}

/// The host of `environment` told the window `event`: how a request it
/// carried out went, said in the status line, named after its environment
/// while the window shows several, and what to type into one of its
/// sessions, typed into its pane; anything else changes that host's work as
/// [`receive`] says.
pub(crate) fn told(state: &mut ViewState, environment: EnvironmentId, event: Event) -> Vec<Effect> {
    let named = |hosts: &Hosts, text: String| {
        if hosts.each.len() > 1 {
            format!("{}: {text}", hosts.name(&environment))
        } else {
            text
        }
    };
    let status = &mut state.chrome.status;
    match event {
        Event::Told(Told::Notice { text }) => {
            status_line::notice(status, named(&state.hosts, text))
        }
        Event::Told(Told::Failure { text }) => {
            let host = state.hosts.each.entry(environment.clone()).or_default();
            if host.failed.as_ref() == Some(&text) {
                return Vec::new();
            }
            host.failed = Some(text.clone());
            status_line::set(status, Some(named(&state.hosts, text)))
        }
        Event::Type { session, typing } => vec![Effect::TypeInto {
            tab: Tab::Session(environment, session),
            typing,
        }],
        event => {
            let before = listed_ids(&state.hosts, &environment);
            let is_patch = matches!(event, Event::Patch(_));
            // A host is known from the first word it says, whether or not the
            // window was told it shows that environment.
            let host = state.hosts.each.entry(environment.clone()).or_default();
            let mut effects = receive(&environment, host, event);
            // A snapshot, or the first list of a host that had none yet, is
            // the work as a window first finds it, not sessions appearing
            // while the user looks on.
            if is_patch && let Some(before) = before {
                effects.extend(show_asked(state, &environment, &before));
            }
            effects
        }
    }
}

/// The sessions and terminals of an environment, by their ids, as listed
/// before a patch.
struct Listed {
    sessions: Vec<String>,
    terminals: Vec<u128>,
}

/// None before the host has listed anything.
fn listed_ids(hosts: &Hosts, environment: &EnvironmentId) -> Option<Listed> {
    let work = hosts.work(environment)?;
    work.tree.listed.as_ref()?;
    Some(Listed {
        sessions: work.tree.sessions().iter().map(|s| s.id.clone()).collect(),
        terminals: tree::terminals(Some(work)).iter().map(|t| t.id).collect(),
    })
}

/// Open each session or terminal of `environment` not among `before` that was
/// not started in the background: the first one the session in the focused
/// window opened is shown with the keys, and every other is opened behind
/// the shown tab, so the user sees it appear without losing what they type
/// into.
fn show_asked(state: &mut ViewState, environment: &EnvironmentId, before: &Listed) -> Vec<Effect> {
    let Some(work) = state.hosts.work(environment) else {
        return Vec::new();
    };
    let sessions = work.tree.sessions();
    let looked_at = match state.content.windows.focused() {
        Some(Tab::Session(at, id)) if at == environment => sessions
            .iter()
            .find(|s| &s.id == id)
            .and_then(|s| s.delegation),
        _ => None,
    };
    let appeared: Vec<_> = sessions
        .iter()
        .filter(|s| s.show && !before.sessions.contains(&s.id))
        .map(|s| {
            (
                Tab::Session(environment.clone(), s.id.clone()),
                s.parent,
                None,
            )
        })
        .chain(
            tree::terminals(Some(work))
                .iter()
                .filter(|t| t.show && !before.terminals.contains(&t.id))
                .map(|t| {
                    let tab = Tab::Terminal(environment.clone(), t.id);
                    (tab, t.opener, Some(t.cwd.clone()))
                }),
        )
        .collect();
    let mut effects = Vec::new();
    let mut keys_taken = false;
    for (tab, opener, cwd) in appeared {
        if keys_taken || looked_at.is_none() || opener != looked_at {
            state.content.windows.open_behind(tab);
            continue;
        }
        keys_taken = true;
        match (tab, cwd) {
            (Tab::Session(_, id), _) => {
                effects.extend(crate::content::show_session(state, environment.clone(), id));
            }
            (tab, cwd) => {
                let content = &mut state.content;
                content.place = cwd.map(|cwd| Place::at(environment, cwd));
                content.windows.open(tab);
            }
        }
    }
    if keys_taken {
        state.content.focus = crate::content::Area::Console;
    }
    effects
}

/// A patch is applied only on top of the one before it. One the window
/// already holds is dropped; one past a gap means patches were lost, and
/// the window asks the host of `environment` for the whole work again, once.
fn patched(environment: &EnvironmentId, host: &mut Host, patch: WorkPatch) -> Vec<Effect> {
    let Some(work) = host.work.as_mut() else {
        return Vec::new();
    };
    if patch.seq == work.seq.next() {
        apply(work, patch);
        return Vec::new();
    }
    if patch.seq <= work.seq || host.resyncing {
        return Vec::new();
    }
    host.resyncing = true;
    vec![Effect::Host {
        environment: environment.clone(),
        request: Request::Snapshot,
    }]
}

fn apply(work: &mut WorkSnapshot, patch: WorkPatch) {
    for change in patch.changes {
        work.apply(change);
    }
    work.seq = patch.seq;
}

#[cfg(test)]
mod tests;
