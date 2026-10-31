//! What the window knows of each environment's work host: whether they
//! speak, and the work as that host last described it.

use std::collections::BTreeMap;

use adroit_model::{EnvironmentId, Session};
use adroit_protocol::{Event, Request, SPOKEN, Sight, Told, Version, WorkPatch, WorkSnapshot};

use crate::chrome::status_line;
use crate::content::Tab;
use crate::effect::Effect;
use crate::state::ViewState;
use crate::tree::{self, nav};

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
    pub fn session(&self, environment: &EnvironmentId, id: &str) -> Option<(nav::Place, &Session)> {
        nav::find(tree::listed(self.work(environment)), id)
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

    /// `request` sent to the host of every environment the window shows.
    pub(crate) fn ask_each(&self, request: Request) -> Vec<Effect> {
        self.each()
            .map(|(environment, _)| Effect::Host {
                environment: environment.clone(),
                request: request.clone(),
            })
            .collect()
    }

    /// `request` sent to the host of the window's own environment.
    pub(crate) fn ask_own(&self, request: Request) -> Effect {
        Effect::Host {
            environment: self.environment.clone(),
            request,
        }
    }

    /// The window is opened in `environment`; its host until then, which
    /// has heard nothing yet, becomes that environment's.
    pub(crate) fn open(&mut self, environment: EnvironmentId) {
        let host = self.each.remove(&self.environment).unwrap_or_default();
        self.each.entry(environment.clone()).or_insert(host);
        self.environment = environment;
    }
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
        }
        Event::Patch(patch) => return patched(environment, host, patch),
        // Told the window as a whole, not its link; an answer goes to
        // what asked.
        Event::Told(_) | Event::Type { .. } | Event::Answer { .. } => {}
    }
    Vec::new()
}

/// The host of `environment` told the window `event`: how a request it
/// carried out went, said in the status line, and what to type into one of
/// its sessions, typed into its pane; anything else changes that host's work
/// as [`receive`] says.
pub(crate) fn told(state: &mut ViewState, environment: EnvironmentId, event: Event) -> Vec<Effect> {
    let status = &mut state.chrome.status;
    match event {
        Event::Told(Told::Notice { text }) => status_line::notice(status, text),
        Event::Told(Told::Failure { text }) => status_line::set(status, Some(text)),
        Event::Type { session, typing } => vec![Effect::TypeInto {
            tab: Tab::Session(environment, session),
            typing,
        }],
        event => {
            // A host is known from the first word it says, whether or not the
            // window was told it shows that environment.
            let host = state.hosts.each.entry(environment.clone()).or_default();
            receive(&environment, host, event)
        }
    }
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
mod tests {
    use adroit_model::EnvironmentId;
    use adroit_protocol::{Event, Request, Version};

    use super::Link;
    use crate::effect::Effect;
    use crate::msg::Msg;
    use crate::scenario::Scenario;
    use crate::testing::{self, here, listing, session};

    // Events rather than builders, so one case can mix patches and snapshots.
    fn patch(seq: u64) -> Event {
        testing::patch(seq).into()
    }

    fn snapshot(seq: u64) -> Event {
        testing::snapshot().seq(seq).into()
    }

    /// An environment other than the window's own.
    fn there() -> EnvironmentId {
        EnvironmentId::new("there")
    }

    fn asks_snapshot(environment: EnvironmentId) -> Effect {
        Effect::Host {
            environment,
            request: Request::Snapshot,
        }
    }

    fn seq_of(run: &Scenario, environment: &EnvironmentId) -> Option<u64> {
        run.state().hosts.work(environment).map(|w| w.seq.0)
    }

    #[test]
    fn opening_the_window_says_hello_in_every_version_it_speaks_to_its_own_environments_host() {
        let run = Scenario::new().send(Msg::Opened {
            environment: there(),
            home: Default::default(),
        });
        assert!(run.last_effects().contains(&Effect::Host {
            environment: there(),
            request: Request::Hello {
                versions: adroit_protocol::SPOKEN.to_vec(),
            },
        }));
        assert_eq!(run.state().hosts.environment, there());
        assert_eq!(run.state().hosts.own().link, Link::Opening);
    }

    #[test]
    fn opening_another_environment_greets_its_host_once_and_names_it() {
        let run = Scenario::new().send(Msg::EnvironmentOpened {
            environment: there(),
            name: "Work machine".into(),
        });
        assert_eq!(
            run.last_effects(),
            [Effect::Host {
                environment: there(),
                request: Request::Hello {
                    versions: adroit_protocol::SPOKEN.to_vec(),
                },
            }]
        );
        assert_eq!(
            run.state().hosts.get(&there()).map(|h| &h.link),
            Some(&Link::Opening)
        );
        assert_eq!(run.state().hosts.environment, here());
        let renamed = run.send(Msg::EnvironmentOpened {
            environment: there(),
            name: "Renamed".into(),
        });
        assert!(renamed.last_effects().is_empty());
        assert_eq!(renamed.state().hosts.name(&there()), "Renamed");
    }

    #[test]
    fn naming_the_window_s_own_environment_does_not_greet_its_host_again() {
        let run = Scenario::new().send(Msg::EnvironmentOpened {
            environment: here(),
            name: "This computer".into(),
        });
        assert!(run.last_effects().is_empty());
        assert_eq!(run.state().hosts.name(&here()), "This computer");
    }

    #[test]
    fn the_answer_to_hello_decides_the_link_of_the_host_that_answered() {
        let cases = [
            (
                Event::Welcome {
                    version: Version(1),
                },
                Link::Ready {
                    version: Version(1),
                },
            ),
            (
                Event::Welcome {
                    version: Version(99),
                },
                Link::Refused {
                    versions: vec![Version(99)],
                },
            ),
            (
                Event::Refused {
                    versions: vec![Version(7)],
                },
                Link::Refused {
                    versions: vec![Version(7)],
                },
            ),
        ];
        for (event, link) in cases {
            let run = Scenario::new().event_from(there(), event.clone());
            let link_of = |environment| run.state().hosts.get(&environment).map(|h| &h.link);
            assert_eq!(link_of(there()), Some(&link), "{event:?}");
            assert_eq!(link_of(here()), Some(&Link::Opening), "{event:?}");
        }
    }

    #[test]
    fn patches_apply_in_order_and_a_gap_asks_for_the_work_again_once() {
        // (events after a snapshot at 3, seq held, snapshots asked for)
        let cases: Vec<(Vec<Event>, Option<u64>, usize)> = vec![
            (vec![patch(4), patch(5)], Some(5), 0),
            (vec![patch(3)], Some(3), 0),
            (vec![patch(2), patch(4)], Some(4), 0),
            (vec![patch(5)], Some(3), 1),
            (vec![patch(5), patch(6), patch(7)], Some(3), 1),
            (vec![patch(5), snapshot(6), patch(8)], Some(6), 2),
            (vec![patch(5), snapshot(6), patch(7)], Some(7), 1),
        ];
        for environment in [here(), there()] {
            for (events, seq, asked) in cases.clone() {
                let run = Scenario::new()
                    .event_from(environment.clone(), snapshot(3))
                    .events_from(environment.clone(), events.clone());
                assert_eq!(seq_of(&run, &environment), seq, "{environment} {events:?}");
                let asks = run
                    .effects()
                    .iter()
                    .filter(|e| **e == asks_snapshot(environment.clone()))
                    .count();
                assert_eq!(asks, asked, "{environment} {events:?}");
            }
        }
    }

    #[test]
    fn patches_before_the_first_snapshot_wait_for_it() {
        for environment in [here(), there()] {
            let run = Scenario::new().events_from(environment.clone(), [patch(1), patch(5)]);
            assert_eq!(run.state().hosts.work(&environment), None, "{environment}");
            assert!(run.effects().is_empty(), "{environment}");
        }
    }

    #[test]
    fn one_environments_snapshots_and_patches_leave_anothers_work_alone() {
        let run = Scenario::new()
            .event_from(here(), snapshot(3))
            .event_from(there(), snapshot(10))
            .event_from(there(), patch(11))
            .event_from(here(), patch(4));
        assert_eq!(seq_of(&run, &here()), Some(4));
        assert_eq!(seq_of(&run, &there()), Some(11));
    }

    #[test]
    fn a_gap_in_one_environment_asks_only_its_host_for_the_work_again() {
        let run = Scenario::new()
            .event_from(here(), snapshot(3))
            .event_from(there(), snapshot(3))
            .event_from(there(), patch(5))
            .event_from(here(), patch(4));
        let asked: Vec<_> = run
            .effects()
            .iter()
            .filter(|e| {
                matches!(
                    e,
                    Effect::Host {
                        request: Request::Snapshot,
                        ..
                    }
                )
            })
            .cloned()
            .collect();
        assert_eq!(asked, vec![asks_snapshot(there())]);
        assert_eq!(seq_of(&run, &here()), Some(4));
        // One host waiting for its snapshot does not keep another from asking.
        let run = run.event_from(here(), patch(6));
        assert_eq!(run.last_effects(), [asks_snapshot(here())]);
    }

    #[test]
    fn a_session_is_found_only_in_the_environment_it_runs_in() {
        let run = Scenario::new()
            .event_from(here(), listing([session("a").name("here-a")]))
            .event_from(there(), listing([session("b").name("there-b")]));
        let name = |environment: EnvironmentId, id: &str| {
            run.state()
                .hosts
                .session(&environment, id)
                .map(|(_, s)| s.name.clone())
        };
        assert_eq!(name(here(), "a").as_deref(), Some("here-a"));
        assert_eq!(name(there(), "b").as_deref(), Some("there-b"));
        assert_eq!(name(there(), "a"), None);
        assert_eq!(name(here(), "b"), None);
        assert_eq!(name(EnvironmentId::new("elsewhere"), "a"), None);
    }

    #[test]
    fn the_same_session_id_in_two_environments_is_two_sessions() {
        let run = Scenario::new()
            .event_from(here(), listing([session("a").name("here-a")]))
            .event_from(there(), listing([session("a").name("there-a")]));
        let name = |environment: EnvironmentId| {
            run.state()
                .hosts
                .session(&environment, "a")
                .map(|(_, s)| s.name.clone())
        };
        assert_eq!(name(here()).as_deref(), Some("here-a"));
        assert_eq!(name(there()).as_deref(), Some("there-a"));
    }

    #[test]
    fn what_the_host_says_of_a_request_is_said_in_the_status_line_and_typing_goes_to_the_pane() {
        use adroit_protocol::{Told, Typing};

        use crate::content::Tab;

        let said = |told: Told| {
            Scenario::new()
                .event(Event::Told(told))
                .state()
                .chrome
                .status
                .line
                .clone()
        };
        let notice = said(Told::Notice {
            text: "ended fix".into(),
        });
        assert_eq!(notice.as_deref(), Some("ended fix"));
        let failure = said(Told::Failure {
            text: "cannot end fix".into(),
        });
        assert_eq!(failure.as_deref(), Some("cannot end fix"));
        let typing = vec![Typing::Paste {
            text: "/rename fixer".into(),
        }];
        for environment in [here(), there()] {
            let run = Scenario::new().event_from(
                environment.clone(),
                Event::Type {
                    session: "0a".into(),
                    typing: typing.clone(),
                },
            );
            assert!(
                run.last_effects().contains(&Effect::TypeInto {
                    tab: Tab::Session(environment.clone(), "0a".into()),
                    typing: typing.clone(),
                }),
                "{environment}"
            );
        }
    }
}
