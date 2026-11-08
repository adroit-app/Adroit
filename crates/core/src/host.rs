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
    /// The failure the host last told, said once until its work comes
    /// again: a link that cannot reach its host tells it at every try.
    failed: Option<String>,
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
            .map(|(p, s)| (nav::Place::at(environment, p), s))
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
            // A snapshot is the work as a window first finds it, not sessions
            // appearing while the user looks on.
            if is_patch {
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

fn listed_ids(hosts: &Hosts, environment: &EnvironmentId) -> Listed {
    let work = hosts.work(environment);
    Listed {
        sessions: work
            .map(|work| work.tree.sessions().iter().map(|s| s.id.clone()).collect())
            .unwrap_or_default(),
        terminals: tree::terminals(work).iter().map(|t| t.id).collect(),
    }
}

/// Show, with the keys, a session or terminal of `environment` not among
/// `before` whose opener asked that it be shown, when the focused window
/// shows that opener.
fn show_asked(state: &mut ViewState, environment: &EnvironmentId, before: &Listed) -> Vec<Effect> {
    let Some(Tab::Session(looked_at, looked_id)) = state.content.windows.focused() else {
        return Vec::new();
    };
    if looked_at != environment {
        return Vec::new();
    }
    let Some(work) = state.hosts.work(environment) else {
        return Vec::new();
    };
    let sessions = work.tree.sessions();
    let Some(opener) = sessions
        .iter()
        .find(|s| &s.id == looked_id)
        .and_then(|s| s.delegation)
    else {
        return Vec::new();
    };
    let session = sessions
        .iter()
        .find(|s| s.show && s.parent == Some(opener) && !before.sessions.contains(&s.id))
        .map(|s| s.id.clone());
    let terminal = tree::terminals(Some(work))
        .iter()
        .find(|t| t.show && t.opener == Some(opener) && !before.terminals.contains(&t.id))
        .map(|t| (t.id, t.cwd.clone()));
    let effects = match (session, terminal) {
        (Some(shown), _) => crate::content::show_session(state, environment.clone(), shown),
        (None, Some((id, cwd))) => {
            let content = &mut state.content;
            content.place = Some(nav::Place::at(environment, cwd));
            content.windows.open(Tab::Terminal(environment.clone(), id));
            Vec::new()
        }
        (None, None) => return Vec::new(),
    };
    state.content.focus = crate::content::Area::Console;
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

    mod shown {
        use adroit_model::SessionId;
        use adroit_model::testing::SessionBuilder;

        use super::*;
        use crate::content::{Area, Tab};

        fn opener(id: SessionId) -> SessionBuilder {
            session("opener").delegation(id)
        }

        /// The window looking at `looking`, listing `listed`, then the host
        /// listing `appearing` besides.
        fn appears(
            looking: &str,
            listed: Vec<SessionBuilder>,
            appearing: SessionBuilder,
        ) -> Scenario {
            let mut all = listed.clone();
            all.push(appearing);
            Scenario::new()
                .event_from(here(), listing(listed).seq(1))
                .send(testing::show(Tab::Session(here(), looking.into())))
                .event_from(here(), testing::patch(2).listed(listed_of(all)))
        }

        fn listed_of(sessions: Vec<SessionBuilder>) -> adroit_protocol::Listed {
            adroit_protocol::testing::listed()
                .sessions(sessions)
                .build()
        }

        fn focused(run: &Scenario) -> Option<Tab> {
            run.state().content.windows.focused().cloned()
        }

        #[test]
        fn a_session_asked_to_be_shown_opens_with_the_keys_while_its_opener_is_looked_at() {
            let id = SessionId::new();
            let run = appears("opener", vec![opener(id)], session("new").parent(id).show());
            assert_eq!(focused(&run), Some(Tab::Session(here(), "new".into())));
            assert_eq!(run.state().content.focus, Area::Console);
        }

        #[test]
        fn a_session_appearing_while_the_user_looks_elsewhere_is_only_listed() {
            let id = SessionId::new();
            let run = appears(
                "other",
                vec![opener(id), session("other")],
                session("new").parent(id).show(),
            );
            assert_eq!(focused(&run), Some(Tab::Session(here(), "other".into())));
        }

        #[test]
        fn a_session_not_asked_to_be_shown_is_only_listed() {
            let id = SessionId::new();
            let run = appears("opener", vec![opener(id)], session("new").parent(id));
            assert_eq!(focused(&run), Some(Tab::Session(here(), "opener".into())));
        }

        #[test]
        fn a_session_asked_to_be_shown_by_another_opener_is_only_listed() {
            let run = appears(
                "opener",
                vec![opener(SessionId::new())],
                session("new").parent(SessionId::new()).show(),
            );
            assert_eq!(focused(&run), Some(Tab::Session(here(), "opener".into())));
        }

        /// A terminal of id `id` the session of delegation `opener` started,
        /// asking that it be shown.
        fn shown_terminal(id: u128, opener: SessionId) -> adroit_model::TerminalSession {
            adroit_model::TerminalSession {
                opener: Some(opener),
                show: true,
                ..adroit_model::testing::shell(id, "/r/opener")
            }
        }

        /// The window looking at `looking`, listing `listed`, then the host
        /// listing `appearing` besides them.
        fn terminal_appears(
            looking: &str,
            listed: Vec<SessionBuilder>,
            appearing: adroit_model::TerminalSession,
        ) -> Scenario {
            let all = adroit_protocol::testing::listed()
                .sessions(listed.clone())
                .terminal(appearing)
                .build();
            Scenario::new()
                .event_from(here(), listing(listed).seq(1))
                .send(testing::show(Tab::Session(here(), looking.into())))
                .event_from(here(), testing::patch(2).listed(all))
        }

        #[test]
        fn a_terminal_asked_to_be_shown_opens_with_the_keys_while_its_opener_is_looked_at() {
            let id = SessionId::new();
            let run = terminal_appears("opener", vec![opener(id)], shown_terminal(7, id));
            assert_eq!(focused(&run), Some(Tab::Terminal(here(), 7)));
            assert_eq!(run.state().content.focus, Area::Console);
        }

        #[test]
        fn a_terminal_appearing_while_the_user_looks_elsewhere_is_only_listed() {
            let id = SessionId::new();
            let run = terminal_appears(
                "other",
                vec![opener(id), session("other")],
                shown_terminal(7, id),
            );
            assert_eq!(focused(&run), Some(Tab::Session(here(), "other".into())));
        }

        #[test]
        fn a_terminal_not_asked_to_be_shown_or_started_by_another_is_only_listed() {
            let id = SessionId::new();
            let unasked = adroit_model::TerminalSession {
                show: false,
                ..shown_terminal(7, id)
            };
            let run = terminal_appears("opener", vec![opener(id)], unasked);
            assert_eq!(focused(&run), Some(Tab::Session(here(), "opener".into())));
            let run = terminal_appears(
                "opener",
                vec![opener(id)],
                shown_terminal(7, SessionId::new()),
            );
            assert_eq!(focused(&run), Some(Tab::Session(here(), "opener".into())));
        }

        #[test]
        fn a_terminal_listed_again_is_not_shown_again() {
            let id = SessionId::new();
            let both = || {
                adroit_protocol::testing::listed()
                    .sessions([opener(id)])
                    .terminal(shown_terminal(7, id))
                    .build()
            };
            let run = Scenario::new()
                .event_from(here(), testing::snapshot().listed(both()).seq(1))
                .send(testing::show(Tab::Session(here(), "opener".into())))
                .event_from(here(), testing::patch(2).listed(both()));
            assert_eq!(focused(&run), Some(Tab::Session(here(), "opener".into())));
        }

        #[test]
        fn a_session_listed_again_is_not_shown_again() {
            let id = SessionId::new();
            let both = || vec![opener(id), session("new").parent(id).show()];
            let run = Scenario::new()
                .event_from(here(), listing(both()).seq(1))
                .send(testing::show(Tab::Session(here(), "opener".into())))
                .event_from(here(), testing::patch(2).listed(listed_of(both())));
            assert_eq!(focused(&run), Some(Tab::Session(here(), "opener".into())));
        }
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

    #[test]
    fn a_failure_a_host_repeats_is_said_once_until_its_work_comes_again() {
        use adroit_protocol::Told;

        let failure = || {
            Event::Told(Told::Failure {
                text: "cannot reach the work host".into(),
            })
        };
        let line = |run: &Scenario| run.state().chrome.status.line.clone();
        let repeated = Scenario::new()
            .event(failure())
            .send(Msg::Status(None))
            .event(failure());
        assert_eq!(line(&repeated), None);
        let again = repeated.event(listing([session("a")])).event(failure());
        assert_eq!(line(&again).as_deref(), Some("cannot reach the work host"));
    }

    #[test]
    fn a_host_s_failure_is_named_after_its_environment_while_several_are_shown() {
        use adroit_protocol::Told;

        let run = Scenario::new()
            .send(Msg::EnvironmentOpened {
                environment: there(),
                name: "There".into(),
            })
            .event_from(
                there(),
                Event::Told(Told::Failure {
                    text: "cannot reach the work host".into(),
                }),
            );
        assert_eq!(
            run.state().chrome.status.line.as_deref(),
            Some("There: cannot reach the work host")
        );
    }
}
