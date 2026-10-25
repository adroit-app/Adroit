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
mod tests;
