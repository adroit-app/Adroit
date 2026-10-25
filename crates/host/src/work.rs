//! The work host: the work it holds, and how it answers the requests of
//! each window that shows it.

use std::collections::{BTreeMap, BTreeSet};

use std::path::PathBuf;

use adroit_model::{Agent, Session, TerminalProgram, TerminalSession};
use adroit_protocol::{
    AgentFacts, AgentsWork, Change, ConfigWork, ConsoleWork, DetailsChange, DetailsWork, Event,
    PastChange, PastWork, PickChange, PickWork, PlaceChange, PlaceWork, PromptChange, PromptWork,
    Request, SPOKEN, Seq, Sight, TreeChange, TreeWork, WorkPatch, WorkSnapshot, agree,
};
use tokio::sync::{mpsc, watch};

/// Everything the host holds of the work.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Work {
    seq: Seq,
    agents: AgentsWork,
    details: DetailsWork,
    tree: TreeWork,
    past: PastWork,
    pick: PickWork,
    place: PlaceWork,
    prompt: PromptWork,
    config: ConfigWork,
    console: ConsoleWork,
}

impl Work {
    /// The work as a window is first shown it.
    pub fn snapshot(&self) -> WorkSnapshot {
        WorkSnapshot {
            seq: self.seq,
            agents: self.agents.clone(),
            details: self.details.clone(),
            tree: self.tree.clone(),
            past: self.past.clone(),
            pick: self.pick.clone(),
            place: self.place.clone(),
            prompt: self.prompt.clone(),
            config: self.config.clone(),
            console: self.console.clone(),
        }
    }

    /// The work before anything is read but the configuration, with what
    /// the agents' adapters tell of them.
    pub fn configured(config: ConfigWork) -> Self {
        Self {
            agents: agent_facts(),
            config,
            ..Self::default()
        }
    }

    pub fn details(&self) -> &DetailsWork {
        &self.details
    }

    pub fn tree(&self) -> &TreeWork {
        &self.tree
    }

    /// The work with `change` made; whether that changed it.
    fn apply(&mut self, change: Change) -> bool {
        match change {
            Change::Details(change) => self.details.apply(change),
            Change::Tree(change) => self.tree.apply(change),
            Change::Past(change) => self.past.apply(change),
            Change::Pick(change) => self.pick.apply(change),
            Change::Place(change) => self.place.apply(change),
            Change::Prompt(change) => self.prompt.apply(change),
            Change::Config(change) => self.config.apply(change),
            Change::Console(change) => self.console.apply(change),
        }
    }

    /// The work a window that shows nothing yet is first sent: what every
    /// window is sent whatever it shows, and nothing read for what another
    /// window shows or asked.
    fn common(&self) -> Work {
        Work {
            seq: self.seq,
            agents: self.agents.clone(),
            details: DetailsWork {
                sessions: BTreeMap::new(),
                repositories: BTreeMap::new(),
                ..self.details.clone()
            },
            tree: TreeWork {
                marks: BTreeMap::new(),
                entries: BTreeMap::new(),
                ..self.tree.clone()
            },
            config: self.config.clone(),
            ..Work::default()
        }
    }
}

/// What each agent's adapter tells of it.
fn agent_facts() -> AgentsWork {
    let agents = Agent::ALL
        .into_iter()
        .map(|agent| {
            let adapter = adroit_agents::adapter(agent);
            AgentFacts {
                agent,
                program: adapter.program(),
                usage_url: adapter.usage_url().map(String::from),
                usage_limits: adapter.capabilities().usage_limits,
            }
        })
        .collect();
    AgentsWork { agents }
}

/// A window the host serves, by the number its connection was given.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct WindowId(pub u64);

/// An event, with the window it is for.
pub type Sent = (WindowId, Event);

/// A window's request for the prompts an agent kept as sent in a project.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HistoryAsk {
    pub window: WindowId,
    pub asked: u64,
    pub agent: Agent,
    pub home: Option<PathBuf>,
    pub project: PathBuf,
}

/// A window drew the screen of `session`, whose prompt goes to `agent`,
/// anew.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Redrawn {
    pub window: WindowId,
    pub session: String,
    pub agent: Agent,
}

/// What a window shows, with the session it shows as the host lists it.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Seen {
    pub sight: Sight,
    pub session: Option<Session>,
}

/// What the host holds for one window: the work as that window was sent
/// it, what it shows, and when it last saw the user type into each
/// session.
#[derive(Debug, Clone, Default)]
struct Shown {
    work: Work,
    seen: Seen,
    inputs: BTreeMap<u128, u64>,
}

impl Shown {
    fn new(work: Work) -> Self {
        Self {
            work,
            ..Self::default()
        }
    }
}

#[derive(Debug)]
pub struct Host {
    /// What every window is sent, and what was read for what any window
    /// shows.
    work: Work,
    /// Each window the host serves.
    windows: BTreeMap<WindowId, Shown>,
    /// What every window shows, in the order the windows came, which the
    /// host keeps read.
    seen: watch::Sender<Vec<Seen>>,
    /// Raised each time a window asks for everything read again.
    again: watch::Sender<u64>,
    /// Raised each time a window asks for the sessions listed again.
    relist: watch::Sender<u64>,
    /// Raised each time a window asks for the commands `/` offers again.
    commands: watch::Sender<u64>,
    /// When the user last typed into each session any window shows, by its
    /// terminal.
    inputs: watch::Sender<BTreeMap<u128, u64>>,
    /// The histories windows ask for, and where they wait until the host's
    /// watchers start.
    history: mpsc::UnboundedSender<HistoryAsk>,
    histories: Option<mpsc::UnboundedReceiver<HistoryAsk>>,
    /// The screens windows drew anew, and where they wait until the host's
    /// watchers start.
    redrawn: mpsc::UnboundedSender<Redrawn>,
    redraws: Option<mpsc::UnboundedReceiver<Redrawn>>,
    /// What windows asked carried out in the background, each with the
    /// window that asked, and where they wait until the host's watchers
    /// start.
    jobs: mpsc::UnboundedSender<(WindowId, Request)>,
    queued: Option<mpsc::UnboundedReceiver<(WindowId, Request)>>,
    /// The terminals of the Neovims to ask to quit, and where they wait
    /// until the host's watchers start.
    quit: mpsc::UnboundedSender<u128>,
    quits: Option<mpsc::UnboundedReceiver<u128>>,
}

impl Default for Host {
    fn default() -> Self {
        Self::new(Work::default())
    }
}

impl Host {
    pub fn new(work: Work) -> Self {
        let (jobs, queued) = mpsc::unbounded_channel();
        let (history, histories) = mpsc::unbounded_channel();
        let (redrawn, redraws) = mpsc::unbounded_channel();
        let (quit, quits) = mpsc::unbounded_channel();
        Self {
            work,
            windows: BTreeMap::new(),
            seen: watch::Sender::new(Vec::new()),
            again: watch::Sender::new(0),
            relist: watch::Sender::new(0),
            commands: watch::Sender::new(0),
            inputs: watch::Sender::new(BTreeMap::new()),
            history,
            histories: Some(histories),
            redrawn,
            redraws: Some(redraws),
            jobs,
            queued: Some(queued),
            quit,
            quits: Some(quits),
        }
    }

    pub fn work(&self) -> &Work {
        &self.work
    }

    /// What every window shows, as it changes.
    pub fn seen(&self) -> watch::Receiver<Vec<Seen>> {
        self.seen.subscribe()
    }

    /// Each time a window asks for everything read again.
    pub fn again(&self) -> watch::Receiver<u64> {
        self.again.subscribe()
    }

    /// Each time a window asks for the sessions listed again.
    pub fn relist(&self) -> watch::Receiver<u64> {
        self.relist.subscribe()
    }

    /// Each time a window asks for the commands `/` offers again.
    pub fn commands_wanted(&self) -> watch::Receiver<u64> {
        self.commands.subscribe()
    }

    /// When the user last typed into each session, the latest any window
    /// said.
    pub fn inputs(&self) -> watch::Receiver<BTreeMap<u128, u64>> {
        self.inputs.subscribe()
    }

    /// The requests to carry out in the background, each with the window
    /// that asked, taken once by what carries them out.
    pub(crate) fn take_jobs(&mut self) -> Option<mpsc::UnboundedReceiver<(WindowId, Request)>> {
        self.queued.take()
    }

    /// The histories windows ask for, taken once by what reads them.
    pub(crate) fn take_histories(&mut self) -> Option<mpsc::UnboundedReceiver<HistoryAsk>> {
        self.histories.take()
    }

    /// The screens windows draw anew, taken once by what looks at them.
    pub(crate) fn take_redraws(&mut self) -> Option<mpsc::UnboundedReceiver<Redrawn>> {
        self.redraws.take()
    }

    /// The terminals of the Neovims to ask to quit, once.
    pub(crate) fn take_quits(&mut self) -> Option<mpsc::UnboundedReceiver<u128>> {
        self.quits.take()
    }

    /// List the sessions again now, as after one was ended or renamed.
    pub(crate) fn ask_relist(&self) {
        self.relist.send_modify(|n| *n += 1);
    }

    /// Read again now what the windows show, as after a process was killed.
    pub(crate) fn ask_again(&self) {
        self.again.send_modify(|n| *n += 1);
    }

    /// What the host tells the windows for `request`, which `window` asked.
    pub fn handle(&mut self, window: WindowId, request: Request) -> Vec<Sent> {
        if !matches!(request, Request::Hello { .. }) {
            // A window that asks before it says hello is served all the
            // same, as one that shows nothing yet.
            self.shown(window);
        }
        match request {
            Request::Hello { versions } => match agree(SPOKEN, &versions) {
                Some(version) => {
                    let shown = Shown::new(self.work.common());
                    let snapshot = shown.work.snapshot();
                    self.windows.insert(window, shown);
                    self.publish();
                    vec![
                        (window, Event::Welcome { version }),
                        (window, Event::Snapshot(Box::new(snapshot))),
                    ]
                }
                None => vec![(
                    window,
                    Event::Refused {
                        versions: SPOKEN.to_vec(),
                    },
                )],
            },
            Request::Snapshot => {
                let snapshot = self.shown(window).work.snapshot();
                vec![(window, Event::Snapshot(Box::new(snapshot)))]
            }
            Request::Watch(sight) => self.watch(window, *sight),
            Request::ReadAgain => {
                self.ask_again();
                Vec::new()
            }
            Request::Relist => {
                self.ask_relist();
                Vec::new()
            }
            job @ (Request::Stop { .. }
            | Request::Kill { .. }
            | Request::Rename { .. }
            | Request::Release { .. }
            | Request::EndTerminal { .. }) => {
                // Carried out by the host's watchers; the window that
                // asked has gone when nothing takes it.
                let _ = self.jobs.send((window, job));
                Vec::new()
            }
            Request::Redrawn { session, agent } => {
                let _ = self.redrawn.send(Redrawn {
                    window,
                    session,
                    agent,
                });
                Vec::new()
            }
            Request::CommandsWanted => {
                self.commands.send_modify(|n| *n += 1);
                Vec::new()
            }
            Request::Compose {
                asked,
                agent,
                text,
                images,
            } => {
                let composed = crate::compose::compose(asked, agent, &text, &images);
                let composed = PromptChange::Composed { composed };
                self.record_for(window, [composed]).into_iter().collect()
            }
            Request::Typed { inputs } => {
                self.shown(window).inputs = inputs;
                self.publish_inputs();
                Vec::new()
            }
            Request::TerminalOpened { terminal } => self.terminal_opened(terminal),
            Request::TerminalClosed { id } => self.terminal_closed(id),
            Request::History {
                asked,
                agent,
                home,
                project,
            } => {
                let _ = self.history.send(HistoryAsk {
                    window,
                    asked,
                    agent,
                    home,
                    project,
                });
                Vec::new()
            }
            // Answered by the connection it came on, which waits for it.
            Request::Ask { .. } => Vec::new(),
        }
    }

    /// The window `window` is gone: nothing more is read for what it showed
    /// alone, and what it typed no longer counts.
    pub fn leave(&mut self, window: WindowId) {
        if self.windows.remove(&window).is_none() {
            return;
        }
        self.publish();
        self.publish_inputs();
        self.prune();
    }

    /// What the host holds for `window`, which a window that asks before it
    /// says hello comes to hold too.
    fn shown(&mut self, window: WindowId) -> &mut Shown {
        let work = &self.work;
        self.windows
            .entry(window)
            .or_insert_with(|| Shown::new(work.common()))
    }

    /// `window` shows `sight` now: what it no longer shows is dropped from
    /// its work, and from the host's once no window shows it.
    fn watch(&mut self, window: WindowId, sight: Sight) -> Vec<Sent> {
        let before = self.terminals_shown();
        let session = self.listed(sight.session.as_deref());
        let shown = self.shown(window);
        shown.seen = Seen { sight, session };
        let left = left(&shown.work, &[&shown.seen.sight]);
        let sent = self.patch_window(window, left).into_iter().collect();
        self.quit_unshown(&before);
        self.publish();
        self.prune();
        sent
    }

    /// The terminals any window shows.
    fn terminals_shown(&self) -> BTreeSet<u128> {
        self.windows
            .values()
            .flat_map(|s| s.seen.sight.terminals.iter().copied())
            .collect()
    }

    /// Ask each Neovim started on a file that a window of `before` showed,
    /// and none shows now, to quit. Neither a window going nor a Neovim
    /// started on no file ends one: the user ends those.
    fn quit_unshown(&self, before: &BTreeSet<u128>) {
        let now = self.terminals_shown();
        let on_file =
            |id: &u128| {
                self.work.tree.terminals().iter().any(|t| {
                    t.id == *id && t.program == TerminalProgram::Neovim && t.file.is_some()
                })
            };
        for id in before.difference(&now).filter(|id| on_file(id)) {
            // None takes it once the watchers are gone with the host.
            let _ = self.quit.send(*id);
        }
    }

    /// Tell the watchers what every window shows now.
    fn publish(&self) {
        let now: Vec<Seen> = self.windows.values().map(|s| s.seen.clone()).collect();
        self.seen.send_if_modified(|was| {
            let changed = *was != now;
            *was = now;
            changed
        });
    }

    /// Tell the lister the latest typing any window saw into each session.
    fn publish_inputs(&self) {
        let mut now = BTreeMap::new();
        for (id, at) in self.windows.values().flat_map(|s| &s.inputs) {
            let latest = now.entry(*id).or_insert(*at);
            *latest = (*latest).max(*at);
        }
        self.inputs.send_if_modified(|was| {
            let changed = *was != now;
            *was = now;
            changed
        });
    }

    /// Drop from the host's work what was read for what no window shows.
    fn prune(&mut self) {
        let sights: Vec<&Sight> = self.windows.values().map(|s| &s.seen.sight).collect();
        let left = left(&self.work, &sights);
        self.patch_shared(left);
    }

    /// The session listed by `id`.
    fn listed(&self, id: Option<&str>) -> Option<Session> {
        let id = id?;
        self.work
            .tree
            .sessions()
            .iter()
            .find(|s| s.id == id)
            .cloned()
    }

    /// The window `window` opened `terminal`, which every window sees in
    /// its tree at once rather than when the sessions are next listed.
    pub fn terminal_opened(&mut self, terminal: TerminalSession) -> Vec<Sent> {
        let id = terminal.id;
        self.terminals(|terminals| {
            if !terminals.iter().any(|t| t.id == id) {
                terminals.push(terminal.clone());
            }
        })
    }

    /// The terminal `id` ended, which every window's tree leaves out at
    /// once.
    pub fn terminal_closed(&mut self, id: u128) -> Vec<Sent> {
        self.terminals(|terminals| terminals.retain(|t| t.id != id))
    }

    /// The terminals listed as `change` leaves them, placed in every
    /// window's tree.
    fn terminals(&mut self, change: impl Fn(&mut Vec<TerminalSession>)) -> Vec<Sent> {
        let placed = |work: &Work| {
            let mut listed = work.tree.listed.clone().unwrap_or_default();
            change(&mut listed.terminals);
            listed.tree = listed.tree.with_terminals(&listed.terminals);
            Change::from(TreeChange::Listed { listed })
        };
        let shared = placed(&self.work);
        self.patch_shared(vec![shared]);
        let ids: Vec<WindowId> = self.windows.keys().copied().collect();
        ids.into_iter()
            .filter_map(|id| {
                let change = placed(&self.windows[&id].work);
                self.patch_window(id, vec![change])
            })
            .collect()
    }

    /// The host read `changes`: each window is sent, as its next patch,
    /// those of what it shows that change its work. What was read for a
    /// session or worktree no window shows any longer is dropped.
    pub fn record<C: Into<Change>>(&mut self, changes: impl IntoIterator<Item = C>) -> Vec<Sent> {
        self.record_to(changes, |_| true)
    }

    /// As [`Host::record`], for the windows whose sight `to` takes alone.
    pub fn record_to<C: Into<Change>>(
        &mut self,
        changes: impl IntoIterator<Item = C>,
        to: impl Fn(&Sight) -> bool,
    ) -> Vec<Sent> {
        let changes: Vec<Change> = changes.into_iter().map(Into::into).collect();
        let shared: Vec<Change> = changes
            .iter()
            .filter(|c| {
                common(c)
                    || self
                        .windows
                        .values()
                        .any(|w| to(&w.seen.sight) && in_sight(c, &w.seen.sight))
            })
            .cloned()
            .collect();
        let relisted = shared
            .iter()
            .any(|c| matches!(c, Change::Tree(TreeChange::Listed { .. })));
        self.patch_shared(shared);
        if relisted {
            self.relisted();
        }
        let ids: Vec<WindowId> = self
            .windows
            .iter()
            .filter(|(_, w)| to(&w.seen.sight))
            .map(|(id, _)| *id)
            .collect();
        ids.into_iter()
            .filter_map(|id| {
                let sight = &self.windows[&id].seen.sight;
                let shown = changes
                    .iter()
                    .filter(|c| in_sight(c, sight))
                    .cloned()
                    .collect();
                self.patch_window(id, shown)
            })
            .collect()
    }

    /// `changes` read for what `window` alone asked, as its next patch.
    pub fn record_for<C: Into<Change>>(
        &mut self,
        window: WindowId,
        changes: impl IntoIterator<Item = C>,
    ) -> Option<Sent> {
        self.patch_window(window, changes.into_iter().map(Into::into).collect())
    }

    /// The session each window shows, as the sessions are listed now.
    fn relisted(&mut self) {
        let ids: Vec<WindowId> = self.windows.keys().copied().collect();
        for id in ids {
            let shows = self.windows[&id].seen.sight.session.clone();
            let session = self.listed(shows.as_deref());
            if let Some(shown) = self.windows.get_mut(&id) {
                shown.seen.session = session;
            }
        }
        self.publish();
    }

    /// `changes` made to the host's work.
    fn patch_shared(&mut self, changes: Vec<Change>) {
        let mut changed = false;
        for change in changes {
            changed |= self.work.apply(change);
        }
        if changed {
            self.work.seq = self.work.seq.next();
        }
    }

    /// `changes` made to the work `window` was sent, those that change it
    /// as its next patch.
    fn patch_window(&mut self, window: WindowId, changes: Vec<Change>) -> Option<Sent> {
        let work = &mut self.windows.get_mut(&window)?.work;
        let changes: Vec<Change> = changes
            .into_iter()
            .filter(|c| work.apply(c.clone()))
            .collect();
        if changes.is_empty() {
            return None;
        }
        work.seq = work.seq.next();
        Some((
            window,
            Event::Patch(WorkPatch {
                seq: work.seq,
                changes,
            }),
        ))
    }
}

/// What `work` holds of sessions and worktrees none of `sights` shows, to
/// drop.
fn left(work: &Work, sights: &[&Sight]) -> Vec<Change> {
    let shown = |c: &Change| sights.iter().any(|s| in_sight(c, s));
    let sessions = work
        .details
        .sessions
        .keys()
        .cloned()
        .map(|session| DetailsChange::SessionLeft { session });
    let repositories = work
        .details
        .repositories
        .keys()
        .cloned()
        .map(|worktree| DetailsChange::RepositoryLeft { worktree });
    let marks = work
        .tree
        .marks
        .keys()
        .cloned()
        .map(|worktree| TreeChange::MarksLeft { worktree });
    let entries = work
        .tree
        .entries
        .keys()
        .cloned()
        .map(|dir| TreeChange::EntriesLeft { dir });
    let past = work
        .past
        .listed
        .as_ref()
        .filter(|(of, _)| !sights.iter().any(|s| s.past.as_ref() == Some(of)))
        .map(|_| Change::Past(PastChange::Left));
    let pick = work
        .pick
        .offers
        .as_ref()
        .filter(|_| sights.iter().all(|s| s.pick.is_none()))
        .map(|_| Change::Pick(PickChange::Left));
    let place = work
        .place
        .read
        .as_ref()
        .filter(|r| !sights.iter().any(|s| s.place.as_ref() == Some(&r.worktree)))
        .map(|_| Change::Place(PlaceChange::Left));
    sessions
        .chain(repositories)
        .map(Change::from)
        .chain(marks.chain(entries).map(Change::from))
        .filter(|c| !shown(c))
        .chain(past)
        .chain(pick)
        .chain(place)
        .collect()
}

/// Whether `change` is of what every window is sent, whatever it shows.
fn common(change: &Change) -> bool {
    matches!(
        change,
        Change::Tree(TreeChange::Listed { .. } | TreeChange::Failing { .. })
            | Change::Details(DetailsChange::Usage { .. } | DetailsChange::System { .. })
            | Change::Config(_)
    )
}

/// Whether `change` is of something `sight` shows; one of nothing shown in
/// particular always is.
fn in_sight(change: &Change, sight: &Sight) -> bool {
    match change {
        Change::Details(change) => details_in_sight(change, sight),
        Change::Tree(TreeChange::Listed { .. } | TreeChange::Failing { .. }) => true,
        Change::Tree(TreeChange::Marks { worktree, .. } | TreeChange::MarksLeft { worktree }) => {
            sight.tree.marked.contains(worktree)
        }
        Change::Tree(TreeChange::Entries { dir, .. } | TreeChange::EntriesLeft { dir }) => {
            sight.listed_dirs().contains(dir)
        }
        Change::Past(PastChange::Listed { sight: of, .. }) => sight.past.as_ref() == Some(of),
        Change::Past(PastChange::Left) => true,
        Change::Pick(PickChange::Offers { .. }) => sight.pick.is_some(),
        Change::Pick(PickChange::Left) => true,
        Change::Place(PlaceChange::Read { read }) => sight.place.as_ref() == Some(&read.worktree),
        Change::Place(PlaceChange::Left) => true,
        Change::Prompt(PromptChange::History { .. } | PromptChange::Composed { .. }) => true,
        Change::Prompt(PromptChange::Commands { read }) => {
            sight.commands.as_ref().is_some_and(|s| read.is_for(s))
        }
        Change::Config(_) => true,
        Change::Console(_) => true,
    }
}

fn details_in_sight(change: &DetailsChange, sight: &Sight) -> bool {
    match change {
        DetailsChange::Conversation { session, .. }
        | DetailsChange::Context { session, .. }
        | DetailsChange::Links { session, .. }
        | DetailsChange::Thumbs { session, .. }
        | DetailsChange::Notes { session, .. }
        | DetailsChange::Processes { session, .. }
        | DetailsChange::SessionLeft { session } => sight.session.as_ref() == Some(session),
        DetailsChange::Repository { worktree, read } => sight
            .repository
            .as_ref()
            .is_some_and(|r| r.worktree == *worktree && r.spot == read.spot),
        DetailsChange::RepositoryLeft { worktree } => sight
            .repository
            .as_ref()
            .is_some_and(|r| r.worktree == *worktree),
        DetailsChange::Usage { .. } | DetailsChange::System { .. } => true,
    }
}

#[cfg(test)]
mod tests {
    use adroit_model::links::Links;
    use adroit_model::repository::{Listing, Spot};
    use adroit_model::testing::{session, shell};
    use adroit_model::{Agent, Past, Session};
    use adroit_protocol::testing::{listed, snapshot};
    use adroit_protocol::{
        Event, Offers, PastChange, PastSight, PastWork, PickSight, RepositoryRead, RepositorySight,
        Request, SPOKEN, Seq, Version,
    };

    use super::*;

    /// The window a host run without one speaks as, in [`scenario`].
    const LONE: WindowId = WindowId(1);

    /// A host run without a window: `requests` in turn, as one window asks
    /// them, the work they leave and every event they draw for that window.
    fn scenario(work: Work, requests: impl IntoIterator<Item = Request>) -> (Work, Vec<Event>) {
        let mut host = Host::new(work);
        let events = requests
            .into_iter()
            .flat_map(|request| host.handle(LONE, request))
            .filter(|(to, _)| *to == LONE)
            .map(|(_, event)| event)
            .collect();
        (host.work, events)
    }

    /// A host as the one window [`LONE`] is served by it.
    trait Lone {
        /// What the host tells [`LONE`] for `request`, which it asked.
        fn on(&mut self, request: Request) -> Vec<Event>;
        /// What [`LONE`] is sent of `changes` read.
        fn read<C: Into<Change>>(&mut self, changes: impl IntoIterator<Item = C>) -> Option<Event>;
    }

    impl Lone for Host {
        fn on(&mut self, request: Request) -> Vec<Event> {
            to(LONE, self.handle(LONE, request))
        }

        fn read<C: Into<Change>>(&mut self, changes: impl IntoIterator<Item = C>) -> Option<Event> {
            to(LONE, self.record(changes)).into_iter().next()
        }
    }

    /// The events of `sent` for `window`.
    fn to(window: WindowId, sent: Vec<Sent>) -> Vec<Event> {
        sent.into_iter()
            .filter(|(to, _)| *to == window)
            .map(|(_, event)| event)
            .collect()
    }

    const A: WindowId = WindowId(1);
    const B: WindowId = WindowId(2);

    /// A host two windows said hello to, each showing the session named.
    fn two_windows(a: &str, b: &str) -> Host {
        let mut host = Host::default();
        for (window, session) in [(A, a), (B, b)] {
            host.handle(window, hello(&[1]));
            host.handle(window, watch(Some(session), None));
        }
        host
    }

    #[test]
    fn what_is_read_reaches_each_window_that_shows_it_and_no_other() {
        let mut host = two_windows("a", "b");
        let sent = host.record([links("a"), links("b")]);
        assert_eq!(changes(to(A, sent.clone()).pop()), vec![links("a")]);
        assert_eq!(changes(to(B, sent).pop()), vec![links("b")]);
        host.handle(B, watch(Some("a"), None));
        let sent = host.record([links("a")]);
        assert!(to(A, sent.clone()).is_empty(), "A has it already");
        assert_eq!(changes(to(B, sent).pop()), vec![links("a")]);
    }

    #[test]
    fn each_window_s_patches_follow_on_from_its_own_snapshot() {
        let mut host = two_windows("a", "b");
        let seqs = |events: Vec<Event>| -> Vec<Seq> {
            events
                .into_iter()
                .filter_map(|e| match e {
                    Event::Patch(p) => Some(p.seq),
                    Event::Snapshot(s) => Some(s.seq),
                    _ => None,
                })
                .collect()
        };
        let a = seqs(to(A, host.record([links("a")])));
        let b = seqs(to(B, host.record([links("b")])));
        let late = WindowId(3);
        let snapshot = seqs(to(late, host.handle(late, hello(&[1]))));
        assert_eq!((a, b), (vec![Seq(1)], vec![Seq(1)]));
        let next = seqs(to(
            late,
            host.record([TreeChange::Failing {
                error: Some("gone".into()),
            }]),
        ));
        assert_eq!(next, vec![snapshot[0].next()]);
    }

    #[test]
    fn a_window_said_hello_late_is_sent_what_every_window_is_and_nothing_another_shows() {
        let mut host = two_windows("a", "b");
        host.record([links("a")]);
        host.record([TreeChange::Failing {
            error: Some("no daemon".into()),
        }]);
        let late = WindowId(3);
        let Some(Event::Snapshot(snapshot)) = to(late, host.handle(late, hello(&[1]))).pop() else {
            panic!("a snapshot");
        };
        assert_eq!(snapshot.tree.failing.as_deref(), Some("no daemon"));
        assert!(snapshot.details.sessions.is_empty());
    }

    #[test]
    fn the_sessions_listed_and_the_terminals_opened_reach_every_window() {
        let mut host = two_windows("a", "b");
        let sent = host.handle(
            B,
            Request::TerminalOpened {
                terminal: shell(7, "/w"),
            },
        );
        assert_eq!(tree_changes(to(A, sent.clone())).len(), 1);
        assert_eq!(tree_changes(to(B, sent)).len(), 1);
        let sent = host.record([
            TreeChange::Failing { error: None },
            TreeChange::Failing {
                error: Some("x".into()),
            },
        ]);
        assert_eq!(to(A, sent.clone()).len(), 1);
        assert_eq!(to(B, sent).len(), 1);
    }

    #[test]
    fn what_was_composed_or_read_for_one_window_reaches_it_alone() {
        let mut host = two_windows("a", "a");
        let sent = host.handle(
            A,
            Request::Compose {
                asked: 1,
                agent: Agent::Claude,
                text: "fix".into(),
                images: Vec::new(),
            },
        );
        assert_eq!(to(A, sent.clone()).len(), 1);
        assert!(to(B, sent).is_empty());
        let dialog = adroit_protocol::ConsoleChange::Dialog {
            dialog: adroit_protocol::Dialog {
                session: "a".into(),
                open: true,
            },
        };
        let sent = host.record_for(B, [dialog]);
        assert_eq!(sent.map(|(to, _)| to), Some(B));
        assert!(host.record_for(WindowId(9), [links("a")]).is_none(), "gone");
    }

    #[test]
    fn what_a_window_asks_carried_out_or_read_is_known_by_the_window_that_asked() {
        let mut host = two_windows("a", "b");
        let mut jobs = host.take_jobs().expect("taken once");
        let mut redraws = host.take_redraws().expect("taken once");
        let rename = Request::Rename {
            id: "b".into(),
            name: "fixer".into(),
            was: "fix".into(),
            agent: Agent::Claude,
        };
        host.handle(B, rename.clone());
        assert_eq!(jobs.try_recv().ok(), Some((B, rename)));
        host.handle(
            A,
            Request::Redrawn {
                session: "a".into(),
                agent: Agent::Claude,
            },
        );
        assert_eq!(redraws.try_recv().ok().map(|r| r.window), Some(A));
    }

    #[test]
    fn a_neovim_on_a_file_no_window_shows_any_longer_is_asked_to_quit_and_nothing_else() {
        let mut host = two_windows("a", "b");
        let mut quits = host.take_quits().expect("taken once");
        let on_file = TerminalSession {
            program: TerminalProgram::Neovim,
            file: Some("/w/a.rs".into()),
            ..shell(7, "/w")
        };
        let on_nothing = TerminalSession {
            program: TerminalProgram::Neovim,
            ..shell(8, "/w")
        };
        for terminal in [on_file, on_nothing, shell(9, "/w")] {
            host.handle(A, Request::TerminalOpened { terminal });
        }
        let showing = |ids: &[u128]| {
            Request::Watch(Box::new(Sight {
                terminals: ids.iter().copied().collect(),
                ..Sight::default()
            }))
        };
        host.handle(A, showing(&[7, 8, 9]));
        host.handle(B, showing(&[7]));
        host.handle(A, showing(&[]));
        assert!(quits.try_recv().is_err(), "another window shows it");
        host.handle(B, showing(&[]));
        assert_eq!(quits.try_recv().ok(), Some(7));
        assert!(quits.try_recv().is_err());
        host.handle(A, showing(&[7]));
        host.leave(A);
        assert!(
            quits.try_recv().is_err(),
            "a window gone shows nothing it hid"
        );
    }

    #[test]
    fn the_lister_counts_the_latest_typing_any_window_saw_and_forgets_a_window_gone() {
        let mut host = two_windows("a", "b");
        let inputs = host.inputs();
        host.handle(
            A,
            Request::Typed {
                inputs: BTreeMap::from([(7, 1_000), (8, 5_000)]),
            },
        );
        host.handle(
            B,
            Request::Typed {
                inputs: BTreeMap::from([(7, 3_000)]),
            },
        );
        assert_eq!(*inputs.borrow(), BTreeMap::from([(7, 3_000), (8, 5_000)]));
        host.leave(B);
        assert_eq!(*inputs.borrow(), BTreeMap::from([(7, 1_000), (8, 5_000)]));
    }

    #[test]
    fn the_watchers_see_what_every_window_shows_until_it_goes() {
        let mut host = two_windows("a", "b");
        let seen = host.seen();
        let shown = || -> Vec<Option<String>> {
            seen.borrow()
                .iter()
                .map(|s| s.sight.session.clone())
                .collect()
        };
        assert_eq!(shown(), [Some("a".into()), Some("b".into())]);
        host.record([links("a"), links("b")]);
        host.leave(A);
        assert_eq!(shown(), [Some("b".into())]);
        assert_eq!(
            host.work().details().sessions.keys().collect::<Vec<_>>(),
            ["b"],
            "what A alone showed is no longer kept"
        );
    }

    #[test]
    fn a_repository_read_reaches_the_windows_that_list_it_as_it_was_read() {
        let mut host = Host::default();
        for (window, spot) in [
            (A, Spot::Log),
            (B, Spot::Files(adroit_model::repository::Rev::WorkingCopy)),
        ] {
            host.handle(window, hello(&[1]));
            host.handle(
                window,
                Request::Watch(Box::new(Sight {
                    repository: Some(RepositorySight {
                        worktree: "/w".into(),
                        spot,
                    }),
                    ..Default::default()
                })),
            );
        }
        let sent = host.record([log("/w")]);
        assert_eq!(changes(to(A, sent.clone()).pop()), vec![log("/w")]);
        assert!(to(B, sent).is_empty(), "B lists the changes, not the log");
    }

    #[test]
    fn a_listing_meant_for_some_windows_reaches_those_alone() {
        let mut host = two_windows("a", "b");
        host.handle(
            B,
            Request::Watch(Box::new(Sight {
                tree: adroit_protocol::TreeSight {
                    every_worktree: true,
                    ..Default::default()
                },
                ..Default::default()
            })),
        );
        let listing = TreeChange::Listed {
            listed: Box::default(),
        };
        let sent = host.record_to([listing], |s| s.tree.every_worktree);
        assert!(to(A, sent.clone()).is_empty());
        assert_eq!(tree_changes(to(B, sent)).len(), 1);
    }

    #[test]
    fn what_takes_a_while_is_queued_for_the_background_and_answers_nothing_at_once() {
        let mut host = Host::default();
        let mut jobs = host.take_jobs().expect("taken once");
        assert!(host.take_jobs().is_none());
        let stop = Request::Stop {
            session: Box::new(Session::default()),
        };
        let rename = Request::Rename {
            id: "0a".into(),
            name: "fixer".into(),
            was: "fix".into(),
            agent: Agent::Claude,
        };
        assert!(host.on(stop.clone()).is_empty());
        assert!(host.on(rename.clone()).is_empty());
        assert_eq!(jobs.try_recv().ok(), Some((LONE, stop)));
        assert_eq!(jobs.try_recv().ok(), Some((LONE, rename)));
    }

    #[test]
    fn the_commands_read_reach_the_window_only_for_what_it_has_in_sight() {
        use adroit_protocol::{CommandsRead, CommandsSight};
        let mut host = Host::default();
        let sight = Sight {
            commands: Some(CommandsSight {
                agent: Agent::Claude,
                home: None,
                dir: "/w".into(),
            }),
            ..Default::default()
        };
        host.on(Request::Watch(Box::new(sight)));
        let read = |dir: &str| PromptChange::Commands {
            read: CommandsRead {
                agent: Agent::Claude,
                home: None,
                dir: dir.into(),
                commands: Vec::new(),
            },
        };
        assert!(host.read([read("/elsewhere")]).is_none());
        assert!(host.read([read("/w")]).is_some());
        let wanted = host.commands_wanted();
        assert!(host.on(Request::CommandsWanted).is_empty());
        assert!(wanted.has_changed().unwrap());
    }

    #[test]
    fn the_host_tells_every_agent_s_program_and_usage_as_its_adapter_does() {
        let work = Work::configured(ConfigWork::default()).snapshot();
        for agent in Agent::ALL {
            let adapter = adroit_agents::adapter(agent);
            let facts = work.agents.of(agent).expect("every agent is told of");
            assert_eq!(facts.program, adapter.program());
            assert_eq!(facts.usage_url.as_deref(), adapter.usage_url());
            assert_eq!(facts.usage_limits, adapter.capabilities().usage_limits);
        }
    }

    fn hello(versions: &[u32]) -> Request {
        Request::Hello {
            versions: versions.iter().copied().map(Version).collect(),
        }
    }

    #[test]
    fn the_host_answers_hello_by_the_versions_both_speak() {
        let work = Work {
            seq: Seq(4),
            ..Default::default()
        };
        let cases = [
            (
                hello(&[1]),
                vec![
                    Event::Welcome {
                        version: Version(1),
                    },
                    snapshot().seq(4).into(),
                ],
            ),
            (
                hello(&[99]),
                vec![Event::Refused {
                    versions: SPOKEN.to_vec(),
                }],
            ),
            (
                hello(&[]),
                vec![Event::Refused {
                    versions: SPOKEN.to_vec(),
                }],
            ),
        ];
        for (request, want) in cases {
            let (_, events) = scenario(work.clone(), [request.clone()]);
            assert_eq!(events, want, "{request:?}");
        }
    }

    #[test]
    fn a_window_that_lost_track_is_sent_the_whole_work() {
        let (work, events) = scenario(
            Work {
                seq: Seq(9),
                ..Default::default()
            },
            [Request::Snapshot],
        );
        assert_eq!(events, vec![snapshot().seq(9).into()]);
        assert_eq!(
            work,
            Work {
                seq: Seq(9),
                ..Default::default()
            }
        );
    }

    fn watch(session: Option<&str>, worktree: Option<&str>) -> Request {
        Request::Watch(Box::new(Sight {
            session: session.map(Into::into),
            repository: worktree.map(|w| RepositorySight {
                worktree: w.into(),
                spot: Spot::Log,
            }),
            ..Default::default()
        }))
    }

    fn links(session: &str) -> DetailsChange {
        DetailsChange::Links {
            session: session.into(),
            links: Links::default(),
        }
    }

    fn log(worktree: &str) -> DetailsChange {
        DetailsChange::Repository {
            worktree: worktree.into(),
            read: RepositoryRead {
                spot: Spot::Log,
                listing: Ok(Listing::Log(Vec::new())),
                revision: None,
            },
        }
    }

    fn changes(event: Option<Event>) -> Vec<DetailsChange> {
        match event {
            Some(Event::Patch(patch)) => patch
                .changes
                .into_iter()
                .filter_map(|c| match c {
                    Change::Details(c) => Some(c),
                    Change::Tree(_)
                    | Change::Past(_)
                    | Change::Pick(_)
                    | Change::Place(_)
                    | Change::Prompt(_)
                    | Change::Config(_)
                    | Change::Console(_) => None,
                })
                .collect(),
            _ => Vec::new(),
        }
    }

    #[test]
    fn what_the_host_reads_reaches_the_window_once_and_only_while_shown() {
        let mut host = Host::default();
        host.on(watch(Some("a"), Some("/w")));
        // (read, what the window is sent)
        let cases = [
            (vec![links("a")], vec![links("a")]),
            (vec![links("a")], vec![]),
            (vec![links("b"), log("/v")], vec![]),
            (vec![log("/w"), log("/w")], vec![log("/w")]),
        ];
        for (read, want) in cases {
            assert_eq!(changes(host.read(read.clone())), want, "{read:?}");
        }
        assert_eq!(host.work().snapshot().seq, Seq(2));
    }

    fn listing(dir: Option<&str>) -> Request {
        Request::Watch(Box::new(Sight {
            past: Some(PastSight {
                dir: dir.map(Into::into),
            }),
            ..Default::default()
        }))
    }

    fn pasts(dir: Option<&str>, ids: &[&str]) -> PastChange {
        PastChange::Listed {
            sight: PastSight {
                dir: dir.map(Into::into),
            },
            conversations: ids
                .iter()
                .map(|id| Past {
                    agent: Agent::Claude,
                    id: id.to_string(),
                    cwd: "/r".into(),
                    title: id.to_string(),
                    at_ms: 0,
                    ..Default::default()
                })
                .collect(),
        }
    }

    fn past_changes(events: impl IntoIterator<Item = Event>) -> Vec<PastChange> {
        events
            .into_iter()
            .flat_map(|e| match e {
                Event::Patch(patch) => patch.changes,
                _ => Vec::new(),
            })
            .filter_map(|c| match c {
                Change::Past(c) => Some(c),
                _ => None,
            })
            .collect()
    }

    #[test]
    fn the_conversations_listed_reach_the_window_only_for_the_list_it_shows() {
        let mut host = Host::default();
        host.on(listing(Some("/r")));
        // (listed, what the window is sent)
        let cases = [
            (pasts(Some("/r"), &["a"]), vec![pasts(Some("/r"), &["a"])]),
            (pasts(Some("/r"), &["a"]), vec![]),
            (pasts(None, &["a", "b"]), vec![]),
            (pasts(Some("/q"), &["c"]), vec![]),
        ];
        for (read, want) in cases {
            assert_eq!(past_changes(host.read([read.clone()])), want, "{read:?}");
        }
    }

    #[test]
    fn closing_or_changing_the_list_drops_the_conversations_listed() {
        let mut host = Host::default();
        host.on(listing(Some("/r")));
        host.read([pasts(Some("/r"), &["a"])]);
        assert_eq!(past_changes(host.on(listing(None))), vec![PastChange::Left]);
        host.read([pasts(None, &["a", "b"])]);
        assert_eq!(
            past_changes(host.on(watch(None, None))),
            vec![PastChange::Left]
        );
        assert_eq!(host.work().snapshot().past, PastWork::default());
        assert!(host.on(watch(None, None)).is_empty());
    }

    #[test]
    fn moving_away_drops_what_was_read_for_what_is_no_longer_shown() {
        let mut host = Host::default();
        host.on(watch(Some("a"), Some("/w")));
        host.read([links("a"), log("/w")]);
        let events = host.on(watch(Some("a"), Some("/v")));
        assert_eq!(
            changes(events.into_iter().next()),
            vec![DetailsChange::RepositoryLeft {
                worktree: "/w".into()
            }]
        );
        let events = host.on(watch(None, None));
        assert_eq!(
            changes(events.into_iter().next()),
            vec![DetailsChange::SessionLeft {
                session: "a".into()
            }]
        );
        assert_eq!(host.work().details(), &DetailsWork::default());
        assert!(host.on(watch(None, None)).is_empty());
    }

    #[test]
    fn the_watchers_are_told_what_the_window_shows_and_when_to_read_again() {
        let mut host = Host::default();
        let seen = host.seen();
        let again = host.again();
        host.on(watch(Some("a"), None));
        assert_eq!(seen.borrow()[0].sight.session.as_deref(), Some("a"));
        assert!(!again.has_changed().unwrap());
        assert!(host.on(Request::ReadAgain).is_empty());
        assert!(again.has_changed().unwrap());
    }

    fn tree_changes(events: Vec<Event>) -> Vec<TreeChange> {
        events
            .into_iter()
            .flat_map(|e| match e {
                Event::Patch(patch) => patch.changes,
                _ => Vec::new(),
            })
            .filter_map(|c| match c {
                Change::Tree(c) => Some(c),
                Change::Details(_)
                | Change::Past(_)
                | Change::Pick(_)
                | Change::Place(_)
                | Change::Prompt(_)
                | Change::Config(_)
                | Change::Console(_) => None,
            })
            .collect()
    }

    fn sees(marked: &[&str], listed: &[&str]) -> Request {
        Request::Watch(Box::new(Sight {
            tree: adroit_protocol::TreeSight {
                marked: marked.iter().map(Into::into).collect(),
                listed: listed.iter().map(Into::into).collect(),
                ..Default::default()
            },
            ..Default::default()
        }))
    }

    fn entries(dir: &str) -> TreeChange {
        TreeChange::Entries {
            dir: dir.into(),
            entries: Vec::new(),
        }
    }

    fn marks(worktree: &str) -> TreeChange {
        TreeChange::Marks {
            worktree: worktree.into(),
            marks: Default::default(),
        }
    }

    #[test]
    fn the_sessions_reach_the_window_whatever_it_shows_and_the_files_only_while_listed() {
        let mut host = Host::default();
        host.on(sees(&["/w"], &["/w"]));
        let failing = TreeChange::Failing {
            error: Some("no daemon".into()),
        };
        // (read, what the window is sent)
        let cases = [
            (vec![failing.clone()], vec![failing]),
            (
                vec![entries("/w"), marks("/w")],
                vec![entries("/w"), marks("/w")],
            ),
            (vec![entries("/v"), marks("/v")], vec![]),
        ];
        for (read, want) in cases {
            let got = tree_changes(host.read(read.clone()).into_iter().collect());
            assert_eq!(got, want, "{read:?}");
        }
        let left = tree_changes(host.on(sees(&[], &[])));
        assert_eq!(
            left,
            [
                TreeChange::MarksLeft {
                    worktree: "/w".into()
                },
                TreeChange::EntriesLeft { dir: "/w".into() },
            ]
        );
        assert!(host.work().tree().entries.is_empty());
    }

    #[test]
    fn a_terminal_the_window_opens_or_closes_is_placed_at_once() {
        let mut host = Host::default();
        host.read([TreeChange::Listed {
            listed: Box::new(
                listed()
                    .session(session("a").cwd("/w"))
                    .outside_git()
                    .build(),
            ),
        }]);
        let opened = host.on(Request::TerminalOpened {
            terminal: shell(7, "/w"),
        });
        assert_eq!(tree_changes(opened).len(), 1);
        let ids = |host: &Host| {
            host.work()
                .tree()
                .tree()
                .worktrees()
                .flat_map(|w| w.terminals.iter().map(|t| t.id))
                .collect::<Vec<_>>()
        };
        assert_eq!(ids(&host), [7]);
        assert_eq!(host.work().tree().terminals().len(), 1);
        let again = host.on(Request::TerminalOpened {
            terminal: shell(7, "/w"),
        });
        assert!(
            again.is_empty(),
            "a terminal listed already changes nothing"
        );
        host.on(Request::TerminalClosed { id: 7 });
        assert!(ids(&host).is_empty());
        assert!(host.work().tree().terminals().is_empty());
    }

    #[test]
    fn the_lister_is_told_when_to_list_again_and_what_was_typed() {
        let mut host = Host::default();
        let relist = host.relist();
        let inputs = host.inputs();
        assert!(host.on(Request::Relist).is_empty());
        assert!(relist.has_changed().unwrap());
        let typed = BTreeMap::from([(7, 1_000)]);
        assert!(
            host.on(Request::Typed {
                inputs: typed.clone()
            })
            .is_empty()
        );
        assert!(inputs.has_changed().unwrap());
        assert_eq!(*inputs.borrow(), typed);
    }

    #[test]
    fn the_watchers_read_the_session_shown_as_the_host_lists_it() {
        let mut host = Host::default();
        let seen = host.seen();
        let listing = |session_id: &str| TreeChange::Listed {
            listed: Box::new(
                listed()
                    .session(session("a").session_id(session_id))
                    .build(),
            ),
        };
        host.on(watch(Some("a"), None));
        assert_eq!(seen.borrow()[0].session, None, "not listed yet");
        host.read([listing("first")]);
        let conversation = || {
            seen.borrow()[0]
                .session
                .as_ref()
                .map(|s| s.session_id.clone())
        };
        assert_eq!(conversation().as_deref(), Some("first"));
        host.read([listing("resumed")]);
        assert_eq!(conversation().as_deref(), Some("resumed"));
        host.on(watch(None, None));
        assert_eq!(seen.borrow()[0].session, None);
    }

    fn picking(listed: &[&str]) -> Request {
        Request::Watch(Box::new(Sight {
            pick: Some(PickSight {
                listed: listed.iter().map(Into::into).collect(),
            }),
            ..Default::default()
        }))
    }

    fn offers(recent: &str) -> PickChange {
        PickChange::Offers {
            offers: Offers {
                recent: vec![recent.into()],
                ..Default::default()
            },
        }
    }

    fn patched(events: impl IntoIterator<Item = Event>) -> Vec<Change> {
        events
            .into_iter()
            .flat_map(|e| match e {
                Event::Patch(patch) => patch.changes,
                _ => Vec::new(),
            })
            .collect()
    }

    #[test]
    fn the_picker_s_offers_reach_the_window_while_it_is_open_and_leave_as_it_closes() {
        let mut host = Host::default();
        assert!(patched(host.read([offers("/a")])).is_empty());
        host.on(picking(&[]));
        assert_eq!(
            patched(host.read([offers("/a")])),
            vec![Change::Pick(offers("/a"))]
        );
        assert_eq!(
            patched(host.on(watch(None, None))),
            vec![Change::Pick(PickChange::Left)]
        );
        assert_eq!(host.work().snapshot().pick.offers, None);
    }

    #[test]
    fn a_history_asked_for_is_read_and_reaches_the_window_whatever_it_shows() {
        let mut host = Host::default();
        let mut asked = host.take_histories().expect("taken once");
        let ask = Request::History {
            asked: 1,
            agent: Agent::Claude,
            home: None,
            project: "/w".into(),
        };
        assert!(host.on(ask).is_empty());
        assert_eq!(
            asked.try_recv().ok().map(|a| (a.window, a.asked)),
            Some((LONE, 1))
        );
        let read = PromptChange::History {
            read: adroit_protocol::HistoryRead {
                asked: 1,
                project: "/w".into(),
                sent: vec!["fix".into()],
            },
        };
        assert_eq!(
            patched(host.record_for(LONE, [read.clone()]).map(|(_, e)| e)),
            vec![Change::Prompt(read)]
        );
    }

    #[test]
    fn the_worktree_in_the_middle_is_read_while_it_is_there_and_left_as_it_goes() {
        let mut host = Host::default();
        let read = |worktree: &str| PlaceChange::Read {
            read: adroit_protocol::PlaceRead {
                worktree: adroit_protocol::HostPath::local(worktree),
                files: vec!["a.rs".into()],
                jj: false,
            },
        };
        let middle = |worktree: Option<&str>| {
            Request::Watch(Box::new(Sight {
                place: worktree.map(adroit_protocol::HostPath::local),
                ..Default::default()
            }))
        };
        assert!(patched(host.read([read("/w")])).is_empty());
        host.on(middle(Some("/w")));
        assert!(patched(host.read([read("/v")])).is_empty(), "not shown");
        assert_eq!(
            patched(host.read([read("/w")])),
            vec![Change::Place(read("/w"))]
        );
        assert!(patched(host.on(middle(Some("/w")))).is_empty());
        assert_eq!(
            patched(host.on(middle(Some("/v")))),
            vec![Change::Place(PlaceChange::Left)]
        );
        assert_eq!(host.work().snapshot().place.read, None);
    }

    #[test]
    fn the_directories_the_picker_lists_are_listed_beside_the_tree_s() {
        let mut host = Host::default();
        host.on(picking(&["/home"]));
        // (read, what the window is sent)
        let cases = [
            (entries("/home"), vec![entries("/home")]),
            (entries("/w"), vec![]),
        ];
        for (read, want) in cases {
            let sent = host.read([read.clone()]).into_iter().collect();
            assert_eq!(tree_changes(sent), want, "{read:?}");
        }
        assert_eq!(
            tree_changes(host.on(watch(None, None))),
            vec![TreeChange::EntriesLeft {
                dir: "/home".into()
            }]
        );
    }

    #[test]
    fn the_configuration_read_reaches_every_window_once_whatever_it_shows() {
        use adroit_protocol::{ConfigChange, ConfigFile, FileText};
        let read = |text: &str| ConfigChange::Settings {
            file: ConfigFile {
                path: "/c/settings.toml".into(),
                text: FileText::Read { text: text.into() },
            },
        };
        let configured = Work::configured(ConfigWork {
            settings: Some(match read("zoom = 1.25\n") {
                ConfigChange::Settings { file } => file,
                ConfigChange::Keys { .. } | ConfigChange::Configured { .. } => unreachable!(),
            }),
            keys: None,
            configured: None,
        });
        let mut host = Host::new(configured.clone());
        assert_eq!(
            host.on(hello(&[1])).last(),
            Some(&Event::Snapshot(Box::new(configured.snapshot())))
        );
        assert!(patched(host.read([read("zoom = 1.25\n")])).is_empty());
        assert_eq!(
            patched(host.read([read("zoom = 1.5\n")])),
            vec![Change::Config(read("zoom = 1.5\n"))]
        );
    }
}
