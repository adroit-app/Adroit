//! The details panel as the window holds it: what it describes, which of its
//! tabs it shows and how far into it it has gone, the groups folded or shown
//! whole, and what the panel asks the work host to read.

use std::collections::{HashMap, HashSet};
use std::path::PathBuf;

use adroit_model::config::UsageSettings;
use adroit_model::links::{Kind as LinkKind, Link, Links};
use adroit_model::repository::{Rev, Spot};
use adroit_model::{ConversationStats, EnvironmentId, Note, Process, Session};
use adroit_protocol::{
    Account, AgentFacts, DetailsWork, RepositorySight, Seq, SessionRead, Sight, WorkSnapshot,
};

use super::{
    Component, DetailsLayout, DetailsTab, Drill, Entry, Fact, Gauge, Group, Listing, Placed,
    Related, Shot, Source, Subject, Surface, TaskDetail, TreeCard, View, about, context_gauge,
    file_link, gauges, grouped, newest_first, related, running, task_detail, timeline_entries,
    tree_at,
};
use crate::content::{Opening, Showing, Tab};
use crate::effect::Effect;
use crate::settings::DetailsSettings;
use crate::tree::nav::Item;
use adroit_model::timeline::{Status, Timeline};

/// What the window shows that the details panel reads by.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Shown {
    /// The environment of what is shown, which the session and the files
    /// the panel lists are of.
    pub environment: EnvironmentId,
    /// The session shown in the middle, by its id.
    pub session: Option<String>,
    /// The worktree shown.
    pub worktree: Option<PathBuf>,
    /// Whether the details panel is open.
    pub open: bool,
    /// The account whose usage the panel shows, when what is shown has one.
    pub usage: Option<Account>,
    /// The images the shown session's console drew.
    pub drawn: Vec<Shot>,
    pub settings: DetailsSettings,
    /// What the overview's About says of the session shown.
    pub about: Vec<Fact>,
    /// The row of the tree the panel describes in place of what is shown: an
    /// environment's heading, or a pull request's or an issue's row.
    pub selected: Option<Item>,
    /// What the panel is made of.
    pub layout: DetailsLayout,
}

impl Shown {
    /// What the panel describes.
    pub fn subject(&self) -> Subject {
        match &self.selected {
            Some(Item::Environment(_)) => Subject::Environment,
            Some(Item::GitHubItem(..)) => Subject::Github,
            _ => Subject::Session,
        }
    }

    /// The tabs and the foot of what the panel describes.
    pub fn surface(&self) -> Surface {
        self.layout.surface(self.subject())
    }
}

/// What the window tells the details panel of what it shows, beyond the
/// session and the worktree the tree follows.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Told {
    pub drawn: Vec<Shot>,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct Details {
    pub drill: Drill,
    /// The groups the user folded or unfolded against how they are placed.
    pub toggled: HashSet<Group>,
    /// The entries' groups shown whole, past their first few.
    pub opened: HashSet<Group>,
    /// The timeline's trees the user folded or unfolded against how they
    /// show at first, by the session and the tree.
    pub trees: HashSet<(String, String)>,
    pub shown: Shown,
    /// The tab last shown of each subject.
    tabs: HashMap<Subject, usize>,
    /// The work and the spot the selection was last brought in line with.
    refreshed: Option<(Seq, Option<Spot>)>,
}

/// A key on the details panel's list.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Key {
    Down,
    Up,
    First,
    Last,
    /// Go to, open or show the row selected, as `Enter` does.
    Open,
    /// Open the row selected as `how` says: an image goes where it says,
    /// and any other row opens as with [`Key::Open`] when `how` opens in
    /// place, and not at all otherwise.
    OpenIn(Opening),
    Fold,
    Copy,
    Reveal,
}

/// How the panel scrolls what is not a list.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Scroll {
    Down,
    Up,
    HalfDown,
    HalfUp,
    Top,
    Bottom,
}

/// What is done with an entry.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Use {
    Open(Opening),
    Copy,
    Reveal,
}

/// A component as the tab shown or the foot holds it, with the rows it
/// lists, the first of them at `at` of the panel's list.
#[derive(Debug, Clone, PartialEq)]
pub struct Part {
    pub placed: Placed,
    /// Its place among the components in sight, the foot's after the tab's.
    pub place: usize,
    pub at: usize,
    pub rows: Vec<Entry>,
    /// Whether it is of the foot.
    pub foot: bool,
    /// What all its rows come to, folded or not, as the line that folds
    /// them says it; none for a component that lists no rows.
    pub said: Option<String>,
}

/// The tab shown, as the surface of what the panel describes has it.
pub fn shown_tab(details: &Details) -> Option<DetailsTab> {
    let tabs = details.shown.surface().tabs;
    tabs.get(details.drill.tab()).or(tabs.first()).cloned()
}

/// The components in sight: the shown tab's, then the foot's, with whether
/// each is of the foot.
fn in_sight(details: &Details) -> Vec<(Placed, bool)> {
    let tab = shown_tab(details).map(|t| t.components).unwrap_or_default();
    let foot = details.shown.surface().foot;
    tab.into_iter()
        .map(|p| (p, false))
        .chain(foot.into_iter().map(|p| (p, true)))
        .collect()
}

/// Whether a component of `component` is in sight.
pub fn shows(details: &Details, component: Component) -> bool {
    in_sight(details)
        .iter()
        .any(|(p, _)| p.component == component)
}

/// The group the rows of `component` are listed under, if it lists any.
fn group_of(component: Component) -> Option<Group> {
    Some(match component {
        Component::Timeline => Group::Timeline,
        Component::Running => Group::Running,
        Component::Processes => Group::Processes,
        Component::About => Group::About,
        Component::Sessions => Group::Sessions,
        Component::Messages => Group::Messages,
        Component::Subagents => Group::Agents,
        Component::Artifacts => Group::Artifacts,
        Component::Images => Group::Images,
        Component::Links => Group::Links,
        Component::Files => Group::Files,
        Component::Changes => Group::Changed,
        Component::Log => Group::Log,
        Component::Prompts => Group::Prompts,
        _ => return None,
    })
}

/// Whether `group` starts folded as it is placed in sight.
fn placed_folded(details: &Details, group: Group) -> bool {
    in_sight(details)
        .iter()
        .find(|(p, _)| group_of(p.component) == Some(group))
        .and_then(|(p, _)| p.folded)
        .unwrap_or(false)
}

/// Whether `group` is folded now.
pub fn is_folded(details: &Details, group: Group) -> bool {
    placed_folded(details, group) != details.toggled.contains(&group)
}

/// The window told the panel `told`.
pub(crate) fn tell(details: &mut Details, told: Told) -> Vec<Effect> {
    details.shown.drawn = told.drawn;
    Vec::new()
}

/// The panel shows the usage of `account`, the one the prompt of `tab` goes
/// to, for a session or a draft whose agent the host says has usage limits;
/// none otherwise.
pub(crate) fn follow_usage(
    details: &mut Details,
    tab: Option<&Tab>,
    account: Account,
    facts: Option<&AgentFacts>,
) {
    let shows = matches!(tab, Some(Tab::Session(_, _) | Tab::Draft { .. }))
        && facts.is_some_and(|f| f.usage_limits);
    let usage = shows.then_some(account);
    if details.shown.usage != usage {
        details.shown.usage = usage;
    }
}

/// The panel is open while its side panel is shown, reads by the settings
/// and is made as the layout says. A tab no longer in the layout gives way
/// to the last there is.
pub(crate) fn follow_settings(
    details: &mut Details,
    open: bool,
    settings: &DetailsSettings,
    layout: &DetailsLayout,
) {
    details.shown.open = open;
    if details.shown.settings != *settings {
        details.shown.settings = settings.clone();
    }
    if details.shown.layout != *layout {
        details.shown.layout = layout.clone();
        let tabs = details.shown.surface().tabs.len();
        if details.drill.tab() >= tabs {
            details.drill = Drill::new(tabs.saturating_sub(1));
        }
    }
}

/// The panel describes the environment's heading or the pull request's or
/// issue's row `selected` in the tree, or else what the window shows. Each
/// subject comes back to the tab it was last left on.
pub(crate) fn follow_selected(details: &mut Details, selected: Option<&Item>) {
    let selected = selected.filter(|i| matches!(i, Item::Environment(_) | Item::GitHubItem(..)));
    if details.shown.selected.as_ref() == selected {
        return;
    }
    let was = details.shown.subject();
    details.shown.selected = selected.cloned();
    let now = details.shown.subject();
    if was != now {
        details.tabs.insert(was, details.drill.tab());
        details.drill = Drill::new(details.tabs.get(&now).copied().unwrap_or(0));
    }
}

/// The panel reads by what the focused window shows. What was opened in
/// one worktree means nothing in another, so a new worktree takes the panel
/// back to its tab's top.
pub(crate) fn follow(details: &mut Details, showing: &Showing) {
    let worktree = showing.place.as_ref().map(|p| p.worktree.clone());
    // A file or an image opened from the panel leaves it on the session
    // it came from, so the next one can be picked.
    let session = match &showing.tab {
        Some(Tab::Session(_, id) | Tab::Drawn { session: id, .. }) => Some(id.clone()),
        Some(Tab::Viewer(_, _)) if worktree == details.shown.worktree => {
            details.shown.session.clone()
        }
        _ => None,
    };
    if worktree != details.shown.worktree {
        details.drill = Drill::new(details.drill.tab());
        details.shown.worktree = worktree;
    }
    if let Some(tab) = &showing.tab
        && tab.environment() != &details.shown.environment
    {
        details.shown.environment = tab.environment().clone();
    }
    if session != details.shown.session {
        details.shown.session = session;
    }
}

/// The tab at `tab` chosen, counting from 0; one already shown stays where
/// it is, and one the layout does not have is not shown.
pub(crate) fn show_tab(details: &mut Details, tab: usize) -> Vec<Effect> {
    if tab < details.shown.surface().tabs.len() && details.drill.tab() != tab {
        details.drill = Drill::new(tab);
    }
    Vec::new()
}

/// The tab `delta` along from the one shown, from its top.
pub(crate) fn step_tab(details: &mut Details, delta: isize) -> Vec<Effect> {
    let tabs = details.shown.surface().tabs.len().max(1);
    let to = crate::tree::nav::wrap(details.drill.tab(), delta, tabs);
    details.drill = Drill::new(to);
    Vec::new()
}

/// The first tab holding `component` shown, unless the one shown holds it.
pub(crate) fn show_component(details: &mut Details, component: Component) -> Vec<Effect> {
    if let Some(at) = tab_of(details, component) {
        show_tab(details, at);
    }
    Vec::new()
}

/// The tab that shows `component`: the one shown when it does, or else the
/// first that does.
fn tab_of(details: &Details, component: Component) -> Option<usize> {
    let tabs = details.shown.surface().tabs;
    let holds = |t: &DetailsTab| t.components.iter().any(|p| p.component == component);
    let now = details.drill.tab();
    if tabs.get(now).is_some_and(holds) {
        return Some(now);
    }
    tabs.iter().position(holds)
}

/// Back out to the list the shown row was opened from.
pub(crate) fn back(details: &mut Details) -> Vec<Effect> {
    details.drill.back();
    Vec::new()
}

/// Entry `at` selected, as its menu is opened on it.
pub(crate) fn select(details: &mut Details, work: Option<&WorkSnapshot>, at: usize) -> Vec<Effect> {
    if let Some(l) = list(details, work) {
        details.drill.select(&l, at);
    }
    Vec::new()
}

/// A row of a commit's files clicked: it opens as `Enter` on it would.
pub(crate) fn open_row(
    details: &mut Details,
    work: Option<&WorkSnapshot>,
    at: usize,
) -> Vec<Effect> {
    if let Some(v) = repository(details, work) {
        details.drill.open_at(&v.listing, at);
    }
    Vec::new()
}

/// An entry clicked: it is selected and does what `Enter` on it would, save
/// that a heading folds or unfolds its group where it is.
pub(crate) fn click(details: &mut Details, work: Option<&WorkSnapshot>, at: usize) -> Vec<Effect> {
    let Some(l) = entry_listing(details, work) else {
        return Vec::new();
    };
    details.drill.select(&l, at);
    let Listing::Entries(entries) = &l else {
        return Vec::new();
    };
    match entries.get(at) {
        Some(Entry::Heading { group, folded, .. }) => {
            fold(details, *group, !folded);
            Vec::new()
        }
        Some(Entry::Tree(card)) => {
            fold_tree(details, card, !card.folded);
            Vec::new()
        }
        Some(entry) => opened(details, &l, entry, Opening::Existing),
        None => Vec::new(),
    }
}

/// `key` pressed on the panel: the cursor moves through a list shown and
/// scrolls anything else.
pub(crate) fn press(details: &mut Details, work: Option<&WorkSnapshot>, key: Key) -> Vec<Effect> {
    let shown = list(details, work);
    match (key, &shown) {
        // A row opened from the entries shows no list: the keys scroll,
        // and those that fold go back to the entries.
        (_, Some(Listing::Entries(_))) if details.drill.shows_row() => match key {
            Key::Fold => {
                details.drill.back();
                Vec::new()
            }
            _ => scrolled(key),
        },
        (_, Some(l @ Listing::Entries(entries))) => {
            let chosen = details.drill.selected(l).map(|at| &entries[at]);
            pressed_on_entries(details, work, l, chosen, key)
        }
        (Key::Fold, _) => {
            details.drill.back();
            Vec::new()
        }
        (Key::Open | Key::OpenIn(Opening::Existing), Some(l)) => {
            details.drill.open(l);
            Vec::new()
        }
        (_, Some(l)) => moved(details, l, key),
        (_, None) => scrolled(key),
    }
}

/// The cursor moved through `l` as `key` moves it; the other keys do
/// nothing.
fn moved(details: &mut Details, l: &Listing, key: Key) -> Vec<Effect> {
    match key {
        Key::Down => details.drill.step(l, 1),
        Key::Up => details.drill.step(l, -1),
        Key::First => details.drill.first(l),
        Key::Last => details.drill.last(l),
        Key::Open | Key::OpenIn(_) | Key::Fold | Key::Copy | Key::Reveal => {}
    }
    Vec::new()
}

fn scrolled(key: Key) -> Vec<Effect> {
    let scroll = match key {
        Key::Down => Scroll::Down,
        Key::Up => Scroll::Up,
        Key::First => Scroll::Top,
        Key::Last => Scroll::Bottom,
        _ => return Vec::new(),
    };
    vec![Effect::ScrollDetails(scroll)]
}

fn pressed_on_entries(
    details: &mut Details,
    work: Option<&WorkSnapshot>,
    l: &Listing,
    chosen: Option<&Entry>,
    key: Key,
) -> Vec<Effect> {
    // A heading folds and unfolds as a workspace's row does; a row folds the
    // group it is listed under.
    if let Some(effects) = pressed_on_timeline(details, work, l, chosen, key) {
        return effects;
    }
    let folds = match (chosen, key) {
        (
            Some(Entry::Heading { group, folded, .. }),
            Key::Open | Key::OpenIn(Opening::Existing),
        ) => Some((*group, !folded)),
        (Some(_), Key::Fold) => details
            .drill
            .selected(l)
            .and_then(|at| l.group_at(at))
            .map(|group| (group, true)),
        _ => None,
    };
    let how = match key {
        Key::Open => Some(Use::Open(Opening::Existing)),
        Key::OpenIn(how) => Some(Use::Open(how)),
        Key::Copy => Some(Use::Copy),
        Key::Reveal => Some(Use::Reveal),
        _ => None,
    };
    match (folds, how, chosen) {
        (Some((group, shut)), ..) => {
            fold(details, group, shut);
            if let Some(now) = entry_listing(details, work)
                && let Some(at) = now.heading(group)
            {
                details.drill.select(&now, at);
            }
            Vec::new()
        }
        (None, Some(Use::Open(how)), Some(entry)) => opened(details, l, entry, how),
        (None, Some(how), Some(entry)) => used(details, entry, how),
        (None, Some(_), None) => Vec::new(),
        _ => moved(details, l, key),
    }
}

/// `entry` of `l` opened: the row for a group's rest shows the group whole,
/// a subagent, message, process, task, file changed or change of the log
/// opens to a level of its own, and any other row is used.
fn opened(details: &mut Details, l: &Listing, entry: &Entry, how: Opening) -> Vec<Effect> {
    let in_place = how == Opening::Existing;
    match entry {
        Entry::Image(_) => used(details, entry, Use::Open(how)),
        _ if !in_place => Vec::new(),
        Entry::More { group, .. } => {
            details.opened.insert(*group);
            Vec::new()
        }
        Entry::Agent(_)
        | Entry::Message(_)
        | Entry::Process(_)
        | Entry::Task(_)
        | Entry::Changed(_)
        | Entry::Commit(_)
        | Entry::Prompt(_) => {
            details.drill.open(l);
            Vec::new()
        }
        Entry::Tree(card) => {
            fold_tree(details, card, !card.folded);
            Vec::new()
        }
        entry => used(details, entry, Use::Open(how)),
    }
}

/// Keys on the timeline's rows: a tree's card folds and unfolds as a
/// heading does, and the keys that fold fold the tree a task is in; none
/// for the keys that do as they do on any row.
fn pressed_on_timeline(
    details: &mut Details,
    work: Option<&WorkSnapshot>,
    l: &Listing,
    chosen: Option<&Entry>,
    key: Key,
) -> Option<Vec<Effect>> {
    let Listing::Entries(entries) = l else {
        return None;
    };
    let at = details.drill.selected(l)?;
    let (card, shut) = match (chosen?, key) {
        (Entry::Tree(card), Key::Open | Key::OpenIn(Opening::Existing)) => {
            (card.clone(), !card.folded)
        }
        (Entry::Tree(_) | Entry::Task(_), Key::Fold) => (tree_at(entries, at)?.clone(), true),
        _ => return None,
    };
    fold_tree(details, &card, shut);
    details.drill.aim(&card.key);
    if let Some(now) = entry_listing(details, work) {
        details.drill.refreshed(&now);
    }
    Some(Vec::new())
}

/// The tree of `card` folded, when `shut`, or unfolded.
fn fold_tree(details: &mut Details, card: &TreeCard, shut: bool) {
    let Some(session) = details.shown.session.clone() else {
        return;
    };
    let key = (session, card.id.clone());
    if shut == (card.status == Status::Completed) {
        details.trees.remove(&key);
    } else {
        details.trees.insert(key);
    }
}

/// What the agent of a session recorded, replayed from what was `read`.
fn timeline_of(read: Option<&SessionRead>) -> Timeline {
    Timeline::replay(read.and_then(|r| r.timeline.as_deref()).unwrap_or_default())
}

/// The trees of the session shown the user folded or unfolded.
fn toggled_trees(details: &Details) -> HashSet<String> {
    let session = details.shown.session.as_deref();
    details
        .trees
        .iter()
        .filter(|(s, _)| Some(s.as_str()) == session)
        .map(|(_, tree)| tree.clone())
        .collect()
}

/// `group` folded, when `shut`, or unfolded. A folded group forgets it was
/// shown whole, so it shows its first few when it opens again.
fn fold(details: &mut Details, group: Group, shut: bool) {
    if shut == placed_folded(details, group) {
        details.toggled.remove(&group);
    } else {
        details.toggled.insert(group);
    }
    if shut {
        details.opened.remove(&group);
    }
}

/// What using `entry` the way `how` says asks done.
fn used(details: &Details, entry: &Entry, how: Use) -> Vec<Effect> {
    match entry {
        Entry::Link(link) => use_link(link, how, &details.shown.environment),
        Entry::Image(shot) => use_shot(
            shot,
            how,
            &details.shown.environment,
            details.shown.session.as_deref(),
        ),
        Entry::Session(r) => use_related(r, how, &details.shown.environment),
        Entry::Process(p) => copied(how, &p.command, "Copied the process command".into()),
        Entry::Message(n) => copied(how, &n.body, "Copied the message".into()),
        Entry::Fact(f) => copied(how, &f.value, format!("Copied {}", f.value)),
        Entry::Agent(a) => copied(
            how,
            &a.prompt,
            format!("Copied the prompt of {}", a.description),
        ),
        Entry::Tree(card) => copied(how, &card.title, format!("Copied {}", card.title)),
        Entry::Task(line) => copied(how, &line.title, format!("Copied {}", line.title)),
        Entry::Changed(c) => copied(how, &c.path, format!("Copied {}", c.path)),
        Entry::Commit(c) => copied(how, &c.short, format!("Copied {}", c.short)),
        Entry::Prompt(p) => copied(how, &p.prompt.text, "Copied the prompt".into()),
        Entry::Heading { .. } | Entry::More { .. } => Vec::new(),
    }
}

fn copied(how: Use, text: &str, done: String) -> Vec<Effect> {
    match how {
        Use::Copy => vec![Effect::Copy {
            text: text.to_owned(),
            done,
        }],
        Use::Open(_) | Use::Reveal => Vec::new(),
    }
}

/// Open `link`, read in `environment`, in the app macOS opens it with, copy
/// it, or show its file in the Finder.
fn use_link(link: &Link, how: Use, environment: &EnvironmentId) -> Vec<Effect> {
    match how {
        Use::Open(_) => vec![Effect::Open {
            environment: environment.clone(),
            target: link.target.clone(),
        }],
        Use::Reveal if matches!(link.kind, LinkKind::File(_)) => vec![Effect::Reveal {
            environment: environment.clone(),
            path: link.target.clone(),
        }],
        Use::Reveal => Vec::new(),
        Use::Copy => vec![Effect::Copy {
            text: link.target.clone(),
            done: format!("Copied {}", link.target),
        }],
    }
}

/// Show `related`, a session of `environment`, or copy its name, which
/// Claude Code's messaging addresses it by.
fn use_related(related: &Related, how: Use, environment: &EnvironmentId) -> Vec<Effect> {
    match (how, &related.id) {
        (Use::Open(_), Some(id)) => vec![Effect::ShowSession(environment.clone(), id.clone())],
        (Use::Open(_), None) => vec![Effect::Notice(format!("{} is not running", related.name))],
        (Use::Copy, _) => vec![Effect::Copy {
            text: related.name.clone(),
            done: format!("Copied {}", related.name),
        }],
        (Use::Reveal, _) => Vec::new(),
    }
}

/// Show `shot` in the content where `how` says, a file in its viewer, or copy
/// or reveal its file as one of the files of Resources is. A drawn image is
/// the console's of `session`, both of `environment`.
fn use_shot(
    shot: &Shot,
    how: Use,
    environment: &EnvironmentId,
    session: Option<&str>,
) -> Vec<Effect> {
    match (&shot.source, how) {
        (_, Use::Open(how)) => shot_tab(shot, environment, session)
            .map(|thing| match how {
                Opening::Existing => Effect::Lightbox(thing),
                how => Effect::Show { thing, how },
            })
            .into_iter()
            .collect(),
        (Source::Console { .. }, Use::Copy) => {
            vec![Effect::Notice("A drawn image has no file to copy".into())]
        }
        (Source::Console { .. }, Use::Reveal) => Vec::new(),
        (Source::File { path, touch }, how) => use_link(&file_link(path, *touch), how, environment),
    }
}

/// What shows `shot` in the content: a file in its viewer, or the drawn image
/// of `session`'s console, both of `environment`.
fn shot_tab(shot: &Shot, environment: &EnvironmentId, session: Option<&str>) -> Option<Tab> {
    match &shot.source {
        Source::Console { id, width, height } => session.map(|session| Tab::Drawn {
            environment: environment.clone(),
            session: session.to_owned(),
            id: *id,
            width: *width,
            height: *height,
        }),
        Source::File { path, .. } => Some(Tab::Viewer(environment.clone(), path.into())),
    }
}

/// The images of the session shown, in their order, as the lightbox steps
/// through them. A file of an environment not `local` is left out, as no
/// viewer reads it.
pub fn images(details: &Details, work: Option<&WorkSnapshot>, local: bool) -> Vec<Tab> {
    let read = session_read(details, work);
    let empty = Links::default();
    let links = read.and_then(|r| r.links.as_ref()).unwrap_or(&empty);
    let mut shots = shots(&details.shown.drawn, links, read);
    shots.truncate(details.shown.settings.links_kept);
    let session = details.shown.session.as_deref();
    shots
        .iter()
        .filter(|s| local || matches!(s.source, Source::Console { .. }))
        .filter_map(|s| shot_tab(s, &details.shown.environment, session))
        .collect()
}

/// What was read of the shown session.
fn session_read<'a>(details: &Details, work: Option<&'a WorkSnapshot>) -> Option<&'a SessionRead> {
    let id = details.shown.session.as_ref()?;
    work?.details.sessions.get(id)
}

/// The session shown, as the tree lists it.
pub fn shown_session<'a>(details: &Details, work: Option<&'a WorkSnapshot>) -> Option<&'a Session> {
    let id = details.shown.session.as_ref()?;
    crate::tree::nav::find(work?.tree.tree(), id).map(|(_, s)| s)
}

/// The rows `placed` lists of the session shown, as it is placed: none for a
/// component that lists none, or while no session is shown. One `alone` in
/// its tab never folds.
/// Every row `placed` lists of `group`, unfolded and shown whole.
fn all_rows(
    details: &Details,
    work: Option<&WorkSnapshot>,
    placed: &Placed,
    group: Group,
) -> Vec<Entry> {
    let mut whole = details.clone();
    // Unfolded, whichever way the placement starts.
    if placed_folded(details, group) {
        whole.toggled.insert(group);
    } else {
        whole.toggled.remove(&group);
    }
    whole.opened.insert(group);
    let placed = Placed {
        folded: Some(false),
        ..placed.clone()
    };
    rows_of(&whole, work, &placed, false)
}

fn rows_of(
    details: &Details,
    work: Option<&WorkSnapshot>,
    placed: &Placed,
    alone: bool,
) -> Vec<Entry> {
    let c = placed.component;
    let folded: HashSet<Group> = group_of(c)
        .filter(|g| !alone && is_folded(details, *g))
        .into_iter()
        .collect();
    let opened = &details.opened;
    let d = &details.shown.settings;
    let first = |default: usize| placed.shown.unwrap_or(default).max(1);
    let repository = |spot: Spot| -> Option<View> {
        let worktree = details.shown.worktree.as_ref()?;
        let read = work?.details.repository(worktree, &spot)?;
        Some(View::of(worktree.clone(), read.clone()))
    };
    match c {
        Component::Changes => {
            let rows = match repository(Spot::Files(Rev::WorkingCopy)).map(|v| v.listing) {
                Some(Listing::Files(files)) => files.into_iter().map(Entry::Changed).collect(),
                _ => Vec::new(),
            };
            return grouped(Group::Changed, rows, &folded, opened, first(usize::MAX));
        }
        Component::Log => {
            let rows = match repository(Spot::Log).map(|v| v.listing) {
                Some(Listing::Log(commits)) => commits.into_iter().map(Entry::Commit).collect(),
                _ => Vec::new(),
            };
            return grouped(Group::Log, rows, &folded, opened, first(usize::MAX));
        }
        _ => {}
    }
    let Some(shown) = shown_session(details, work) else {
        return Vec::new();
    };
    let read = session_read(details, work);
    let empty = Links::default();
    let links = read.and_then(|r| r.links.as_ref()).unwrap_or(&empty);
    let ties = || {
        let sessions: Vec<&Session> = work
            .map(|w| w.tree.tree().sessions().map(|(_, s)| s).collect())
            .unwrap_or_default();
        related(shown, &sessions, &links.peers)
    };
    let links_of = |group: Group| -> Vec<Entry> {
        let rows: Vec<Link> = links.rows().cloned().collect();
        super::capped(&rows, d.links_kept)
            .into_iter()
            .filter(|l| Group::of(&l.kind) == group)
            .map(Entry::Link)
            .collect()
    };
    match c {
        Component::Timeline => {
            let shown = if opened.contains(&Group::Timeline) {
                usize::MAX
            } else {
                first(usize::MAX)
            };
            timeline_entries(&timeline_of(read), &toggled_trees(details), shown)
        }
        Component::Running => {
            let processes = read
                .and_then(|r| r.processes.as_deref())
                .unwrap_or_default();
            running(
                &ties(),
                &links.agents,
                processes,
                &folded,
                opened,
                first(d.links_shown),
            )
        }
        Component::Processes => {
            let rows = read
                .and_then(|r| r.processes.as_deref())
                .unwrap_or_default()
                .iter()
                .cloned()
                .map(Entry::Process)
                .collect();
            grouped(
                Group::Processes,
                rows,
                &folded,
                opened,
                first(d.links_shown),
            )
        }
        Component::About => about(&details.shown.about, &folded),
        Component::Sessions => {
            let rows = ties().into_iter().map(Entry::Session).collect();
            grouped(Group::Sessions, rows, &folded, opened, first(usize::MAX))
        }
        Component::Messages => {
            let notes = read.and_then(|r| r.notes.as_deref()).unwrap_or_default();
            let rows = notes.iter().cloned().map(Entry::Message).collect();
            grouped(Group::Messages, rows, &folded, opened, first(d.links_shown))
        }
        Component::Subagents => {
            let rows = links.agents.iter().cloned().map(Entry::Agent).collect();
            grouped(Group::Agents, rows, &folded, opened, first(usize::MAX))
        }
        Component::Artifacts | Component::Links | Component::Files => {
            let group = group_of(c).unwrap_or(Group::Files);
            grouped(
                group,
                links_of(group),
                &folded,
                opened,
                first(d.links_shown),
            )
        }
        Component::Prompts => {
            let rows = links
                .prompts
                .iter()
                .cloned()
                .map(|p| Entry::Prompt(super::PromptRow::new(p)))
                .collect();
            grouped(Group::Prompts, rows, &folded, opened, first(d.links_shown))
        }
        Component::Images => {
            let mut shots = shots(&details.shown.drawn, links, read);
            shots.truncate(d.links_kept);
            let rows = shots.into_iter().map(Entry::Image).collect();
            grouped(Group::Images, rows, &folded, opened, first(d.links_shown))
        }
        _ => Vec::new(),
    }
}

/// The components in sight, each with the rows it lists: the shown tab's,
/// then the foot's. A component alone in its tab lists its rows without a
/// heading over them.
pub fn parts(details: &Details, work: Option<&WorkSnapshot>) -> Vec<Part> {
    let mut at = 0;
    let sight = in_sight(details);
    let in_tab = sight.iter().filter(|(_, foot)| !foot).count();
    sight
        .into_iter()
        .enumerate()
        .map(|(place, (placed, foot))| {
            let alone = !foot && in_tab == 1;
            let mut rows = rows_of(details, work, &placed, alone);
            if alone {
                // The tab's own title names what it lists.
                rows.retain(|e| !matches!(e, Entry::Heading { .. }));
            }
            let said = group_of(placed.component)
                .map(|group| crate::details::said(group, &all_rows(details, work, &placed, group)));
            let part = Part {
                placed,
                place,
                at,
                rows,
                foot,
                said,
            };
            at += part.rows.len();
            part
        })
        .collect()
}

/// `parts` as they are drawn: in lines, each of a part and those beside it;
/// and the lines in runs, those of the machine's meters next to each other
/// drawn together as one, as are those of the agent's gauges, the rest each
/// on its own.
pub fn runs(parts: Vec<Part>) -> Vec<Vec<Vec<Part>>> {
    let mut lines: Vec<Vec<Part>> = Vec::new();
    for part in parts {
        let joins = part.placed.beside
            && lines
                .last()
                .and_then(|l| l.last())
                .is_some_and(|last| last.foot == part.foot);
        match lines.last_mut() {
            Some(line) if joins => line.push(part),
            _ => lines.push(vec![part]),
        }
    }
    // Whether a line is all meters or all gauges, and where it is.
    let family = |line: &[Part]| {
        let all = |f: fn(Component) -> bool| line.iter().all(|p| f(p.placed.component));
        let kind = if all(|c| c.meter().is_some()) {
            Some(true)
        } else if all(|c| c.gauge().is_some()) {
            Some(false)
        } else {
            None
        };
        kind.map(|k| (k, line.first().is_some_and(|p| p.foot)))
    };
    let mut out: Vec<Vec<Vec<Part>>> = Vec::new();
    for line in lines {
        let joins = family(&line).is_some_and(|f| {
            out.last()
                .and_then(|run| run.last())
                .is_some_and(|last| family(last) == Some(f))
        });
        match out.last_mut() {
            Some(run) if joins => run.push(line),
            _ => out.push(vec![line]),
        }
    }
    out
}

/// The rows the components in sight list, in their order; none while a
/// change of the log or a file it changed is opened, or no component lists
/// any.
pub fn entry_listing(details: &Details, work: Option<&WorkSnapshot>) -> Option<Listing> {
    if details.drill.spot().is_some() {
        return None;
    }
    let entries: Vec<Entry> = parts(details, work)
        .into_iter()
        .flat_map(|p| p.rows)
        .collect();
    (!entries.is_empty()).then_some(Listing::Entries(entries))
}

/// The change the worktree shown is on: what was read of the working copy's
/// files, while it was.
pub fn working_copy(details: &Details, work: Option<&WorkSnapshot>) -> Option<View> {
    let worktree = details.shown.worktree.as_ref()?;
    let spot = Spot::Files(Rev::WorkingCopy);
    let read = work?.details.repository(worktree, &spot)?;
    Some(View::of(worktree.clone(), read.clone()))
}

/// What was read of the log of the worktree shown, while it was.
pub fn log(details: &Details, work: Option<&WorkSnapshot>) -> Option<View> {
    let worktree = details.shown.worktree.as_ref()?;
    let read = work?.details.repository(worktree, &Spot::Log)?;
    Some(View::of(worktree.clone(), read.clone()))
}

/// The images: what the console `drawn`, and the image files of `links`
/// there are small copies of in `read`, newest first.
fn shots(drawn: &[Shot], links: &Links, read: Option<&SessionRead>) -> Vec<Shot> {
    let files = links.images.iter().filter_map(|f| {
        let thumb = read?.thumbs.get(&f.path)?;
        Some(Shot::file(&f.path, f.touch, f.at_ms, thumb.clone()))
    });
    newest_first(drawn.iter().cloned().chain(files).collect())
}

/// What was read of the worktree shown for the row opened: a change's files
/// or a file's diff.
pub fn repository(details: &Details, work: Option<&WorkSnapshot>) -> Option<View> {
    let spot = details.drill.spot()?;
    let worktree = details.shown.worktree.as_ref()?;
    let read = work?.details.repository(worktree, &spot)?;
    Some(View::of(worktree.clone(), read.clone()))
}

/// The list the panel's cursor moves through: the components' rows, or the
/// files of a change opened; none while the panel shows no list.
pub fn list(details: &Details, work: Option<&WorkSnapshot>) -> Option<Listing> {
    if details.drill.spot().is_none() {
        return entry_listing(details, work);
    }
    repository(details, work)
        .map(|v| v.listing)
        .filter(Listing::is_list)
}

/// What the conversation of the session shown says of itself, while it is
/// the conversation it goes on in.
pub fn stats(details: &Details, work: Option<&WorkSnapshot>) -> Option<ConversationStats> {
    let shown = shown_session(details, work)?;
    let conversation = session_read(details, work)?.conversation.as_ref()?;
    (conversation.id == shown.session_id).then(|| conversation.stats.clone())
}

/// The gauges of the agent's usage: how full the shown conversation's
/// context is, then the account's limits at `now_ms`.
pub fn usage(details: &Details, work: Option<&WorkSnapshot>, now_ms: u64) -> Vec<Gauge> {
    let mut out = match (&details.shown.usage, work) {
        (Some(account), Some(work)) => work
            .details
            .usage
            .get(account)
            .map(|limits| gauges(limits, now_ms))
            .unwrap_or_default(),
        _ => Vec::new(),
    };
    let context = stats(details, work).and_then(|stats| {
        let shown = shown_session(details, work)?;
        let asked = session_read(details, work)?
            .context
            .as_ref()
            .filter(|c| c.conversation == shown.session_id)
            .map(|c| c.context);
        context_gauge(&ConversationStats {
            context: stats.context.or(asked),
            ..stats
        })
    });
    if let Some(gauge) = context {
        out.insert(0, gauge);
    }
    out
}

/// The subagent opened.
pub fn opened_agent(
    details: &Details,
    work: Option<&WorkSnapshot>,
) -> Option<adroit_model::subagents::Agent> {
    let call = details.drill.agent()?;
    session_read(details, work)?
        .links
        .as_ref()?
        .agents
        .iter()
        .find(|a| a.call == call)
        .cloned()
}

/// The message opened.
pub fn opened_note(details: &Details, work: Option<&WorkSnapshot>) -> Option<Note> {
    let id = details.drill.message()?;
    session_read(details, work)?
        .notes
        .as_ref()?
        .iter()
        .find(|n| n.id == id)
        .cloned()
}

/// The process opened.
pub fn opened_process(details: &Details, work: Option<&WorkSnapshot>) -> Option<Process> {
    let key = details.drill.process()?;
    session_read(details, work)?
        .processes
        .as_ref()?
        .iter()
        .find(|p| p.key == key)
        .cloned()
}

/// The prompt opened.
pub fn opened_prompt(
    details: &Details,
    work: Option<&WorkSnapshot>,
) -> Option<adroit_model::links::Prompt> {
    let turn = details.drill.prompt()?;
    session_read(details, work)?
        .links
        .as_ref()?
        .prompts
        .iter()
        .find(|p| p.turn == turn)
        .cloned()
}

/// The task opened.
pub fn opened_task(details: &Details, work: Option<&WorkSnapshot>) -> Option<TaskDetail> {
    let id = details.drill.task()?;
    task_detail(&timeline_of(session_read(details, work)), id)
}

/// The root of the repository whose pull requests the host keeps read for
/// **github**, while it is in sight of what a window shows.
pub fn pulls_wanted(details: &Details, work: Option<&WorkSnapshot>) -> Option<PathBuf> {
    let shown = details.shown.open
        && details.shown.subject() == Subject::Session
        && in_sight(details)
            .iter()
            .any(|(p, _)| p.component.is_github());
    let worktree = details.shown.worktree.as_ref().filter(|_| shown)?;
    let workspace = crate::tree::listed(work).workspace_of(worktree)?;
    workspace.git.then(|| workspace.root.clone())
}

/// The pull request open for the branch the worktree shown has checked out,
/// as last read: the root of its repository and its number.
pub fn branch_pull(details: &Details, work: Option<&WorkSnapshot>) -> Option<(PathBuf, u64)> {
    let worktree = details.shown.worktree.as_ref()?;
    let workspace = crate::tree::listed(work).workspace_of(worktree)?;
    let branch = workspace
        .worktrees
        .iter()
        .find(|w| &w.worktree.path == worktree)?
        .worktree
        .branch
        .as_ref()?;
    let pulls = crate::tree::github(work, &workspace.root)?
        .read(adroit_model::github::Listing::Pulls)?
        .as_ref()
        .ok()?;
    pulls
        .iter()
        .find(|i| i.pull.as_ref().is_some_and(|p| &p.head == branch))
        .map(|i| (workspace.root.clone(), i.number))
}

/// The machine's meters the components in sight show, together; none while
/// none shows the machine.
pub fn meters(details: &Details) -> Option<UsageSettings> {
    in_sight(details)
        .iter()
        .filter_map(|(p, _)| p.component.meter())
        .reduce(|a, b| a.or(b))
}

/// Whether the components in sight list the environment's profiles, with
/// the usage of every login they keep.
pub fn lists_profiles(details: &Details) -> bool {
    details.shown.open && shows(details, Component::Profiles)
}

/// What the host keeps read for the panel: the shown session's conversation,
/// and, for the components in sight, its links, messages and processes, the
/// spots of the worktree shown and the account's usage.
pub(crate) fn sight(details: &Details) -> Sight {
    let shown = &details.shown;
    let of_window = shown.subject() == Subject::Session;
    let has = |c| of_window && shows(details, c);
    let reads_entries = [
        Component::Prompts,
        Component::Running,
        Component::Processes,
        Component::Sessions,
        Component::Messages,
        Component::Subagents,
        Component::Artifacts,
        Component::Images,
        Component::Links,
        Component::Files,
    ]
    .into_iter()
    .any(has);
    let mut spots = Vec::new();
    if has(Component::Change) || has(Component::Changes) {
        spots.push(Spot::Files(Rev::WorkingCopy));
    }
    if has(Component::Log) {
        spots.push(Spot::Log);
    }
    if let Some(spot) = details.drill.spot().filter(|s| !spots.contains(s)) {
        spots.push(spot);
    }
    let repositories = match &shown.worktree {
        Some(worktree) if of_window => spots
            .into_iter()
            .map(|spot| RepositorySight {
                worktree: worktree.clone(),
                spot,
            })
            .collect(),
        _ => Vec::new(),
    };
    Sight {
        session: shown.session.clone(),
        details: shown.open,
        overview: reads_entries,
        repositories,
        usage: shown.usage.clone().filter(|_| {
            Component::ALL.iter().any(|c| {
                c.gauge()
                    .is_some_and(|g| g != crate::details::GaugeKind::Context)
                    && has(*c)
            })
        }),
        ..Default::default()
    }
}

/// Take in what was read again for the spot shown.
pub(crate) fn refresh(details: &mut Details, work: Option<&WorkSnapshot>) {
    let now = work.map(|w| (w.seq, details.drill.spot()));
    if now.is_none() || details.refreshed == now {
        return;
    }
    details.refreshed = now;
    if let Some(v) = repository(details, work) {
        details.drill.refreshed(&v.listing);
    }
}

/// A subagent, process or task opened that the entries no longer list is
/// gone back from.
pub(crate) fn forget_gone(details: &mut Details, work: Option<&DetailsWork>) {
    let read = details
        .shown
        .session
        .as_ref()
        .and_then(|id| work?.sessions.get(id));
    let gone = details.drill.agent().is_some_and(|call| {
        !read
            .and_then(|r| r.links.as_ref())
            .is_some_and(|l| l.agents.iter().any(|a| a.call == call))
    }) || details.drill.process().is_some_and(|key| {
        !read
            .and_then(|r| r.processes.as_ref())
            .is_some_and(|rows| rows.iter().any(|p| p.key == key))
    }) || details
        .drill
        .task()
        .is_some_and(|id| timeline_of(read).task(id).is_none());
    if gone {
        details.drill.back();
    }
}

#[cfg(test)]
pub(crate) mod tests {
    const OVERVIEW: usize = 0;
    const CHANGES: usize = 1;
    const LOG: usize = 2;
    const AGENTS: usize = 3;
    const RESOURCES: usize = 4;
    const TIMELINE: usize = 5;

    /// The tabs the behaviour below is told against: one sheet each, as the
    /// panel had them before it was arranged, so what Adroit ships may
    /// change without these.
    const CLASSIC: &str = "[panel.session]\n\
        foot = [\"context\", \"session-limit\", \"week-limit\", \"model-limits\", \"cpu\", \"gpu\", \"memory\", \"storage\"]\n\
        [[panel.session.tabs]]\ntitle = \"Overview\"\n\
        components = [\"session\", \"change\", { component = \"timeline\", shown = 3 }, \"running\", { component = \"about\", folded = true }]\n\
        [[panel.session.tabs]]\ntitle = \"Changes\"\ncomponents = [\"changes\"]\n\
        [[panel.session.tabs]]\ntitle = \"Log\"\ncomponents = [\"log\"]\n\
        [[panel.session.tabs]]\ntitle = \"Agents\"\ncomponents = [\"sessions\", \"messages\", \"subagents\"]\n\
        [[panel.session.tabs]]\ntitle = \"Resources\"\ncomponents = [\"artifacts\", \"images\", \"links\", \"files\"]\n\
        [[panel.session.tabs]]\ntitle = \"Timeline\"\ncomponents = [\"timeline\"]\n";

    fn scenario() -> Scenario {
        Scenario::new().event(crate::config::testing::configured(CLASSIC))
    }

    use crate::testing::here;
    use std::collections::BTreeMap;
    use std::sync::Arc;

    use adroit_model::Agent;
    use adroit_model::links::{Peer, Touch};
    use adroit_model::repository::{DiffKind, DiffLine, Listing as Read, Rev};
    use adroit_model::subagents::{Agent as Subagent, Standing};
    use adroit_model::testing::session;
    use adroit_model::usage::{Limit, Severity, Window};
    use adroit_protocol::{
        AgentsWork, ContextRead, ConversationRead, DetailsChange, RepositoryRead, Request,
    };

    use super::*;
    use crate::content::Axis;
    use crate::msg::Msg;
    use crate::scenario::Scenario;
    use crate::testing::{ListedBuilder, SnapshotBuilder, listed, patch, snapshot};
    use crate::tree::nav::Place;

    /// The sessions the host lists, every one these tests show, outside git
    /// in `/w`.
    fn listing() -> ListedBuilder {
        let ids = ["a", "b", "s1", "s2", "session-one", "session-two"];
        listed().outside_git().sessions(ids.map(|id| {
            session(id)
                .cwd("/w")
                .session_id(format!("{id}-conversation"))
        }))
    }

    /// The focused window showing the session `id`, or nothing once it closes,
    /// the middle left at the worktree it was in.
    fn shown(id: Option<&str>, worktree: Option<&str>) -> Msg {
        match id {
            Some(id) => Msg::Show {
                thing: Tab::Session(here(), id.into()),
                place: worktree.map(|w| Place::at(&here(), w)),
                how: None,
            },
            None => Msg::Window(crate::actions::Action::TabClose),
        }
    }

    /// The host's word of `agent`, whose account has usage limits or not.
    fn with_usage(agent: Agent, limits: bool) -> AgentsWork {
        AgentsWork {
            agents: vec![AgentFacts {
                agent,
                program: agent.key().into(),
                usage_url: None,
                usage_limits: limits,
            }],
        }
    }

    fn told(told: Told) -> Msg {
        Msg::DetailsShown(Box::new(told))
    }

    fn link(kind: LinkKind, target: &str) -> Link {
        Link {
            kind,
            target: target.into(),
            title: None,
            line: None,
        }
    }

    /// The host's first word: the sessions listed, and what was read of them.
    fn work(details: DetailsWork) -> SnapshotBuilder {
        snapshot().listed(listing()).details(details)
    }

    fn read(id: &str, read: SessionRead) -> DetailsWork {
        DetailsWork {
            sessions: BTreeMap::from([(id.to_string(), read)]),
            ..Default::default()
        }
    }

    fn with_links(links: Links) -> SessionRead {
        SessionRead {
            links: Some(links),
            ..Default::default()
        }
    }

    fn watched(run: &Scenario) -> Vec<Sight> {
        run.effects()
            .iter()
            .filter_map(|e| match e {
                Effect::Host {
                    request: Request::Watch(sight),
                    ..
                } => Some((**sight).clone()),
                _ => None,
            })
            .collect()
    }

    fn local(run: &Scenario) -> Vec<Effect> {
        run.last_effects()
            .iter()
            .filter(|e| !matches!(e, Effect::Host { .. }))
            .cloned()
            .collect()
    }

    fn entries_of(run: &Scenario) -> Option<Listing> {
        entry_listing(&run.state().details, run.state().hosts.own().work.as_ref())
    }

    #[test]
    fn another_environment_s_tab_has_its_details_read_by_that_environment_s_host() {
        use crate::testing::{shows_there, there};
        let run = scenario().event(snapshot().listed(listing()));
        let run =
            shows_there(run, [session("b").cwd("/t")]).send(Msg::ShowSession(there(), "b".into()));
        let watched_by = |environment: &EnvironmentId| {
            run.state()
                .hosts
                .get(environment)
                .map(|h| h.watched.session.clone())
        };
        assert_eq!(watched_by(&there()), Some(Some("b".into())));
        assert_eq!(watched_by(&here()), Some(None));
        let again = run.send(Msg::DetailsReadAgain);
        assert_eq!(
            again.last_effects(),
            [Effect::Host {
                environment: there(),
                request: Request::ReadAgain,
            }]
        );
    }

    #[test]
    fn the_host_is_asked_to_read_what_the_panel_shows_once_each_time_it_changes() {
        let run = scenario().send_all([
            shown(Some("a"), Some("/w")),
            shown(Some("a"), Some("/w")),
            Msg::DetailsTab(LOG),
            Msg::DetailsTab(LOG),
            shown(None, Some("/w")),
        ]);
        let sights: Vec<_> = watched(&run)
            .into_iter()
            .map(|s| {
                (
                    s.session,
                    s.overview,
                    s.repositories
                        .into_iter()
                        .map(|r| (r.worktree, r.spot))
                        .collect::<Vec<_>>(),
                )
            })
            .collect();
        assert_eq!(
            sights,
            [
                // As the settings are first taken in, with nothing shown.
                (None, true, vec![]),
                (
                    Some("a".into()),
                    true,
                    vec![("/w".into(), Spot::Files(Rev::WorkingCopy))]
                ),
                (Some("a".into()), false, vec![("/w".into(), Spot::Log)]),
                (None, false, vec![("/w".into(), Spot::Log)]),
            ]
        );
    }

    #[test]
    fn a_new_worktree_takes_the_panel_back_to_its_sheet_s_top() {
        let log = RepositoryRead {
            spot: Spot::Log,
            listing: Ok(Read::Log(vec![adroit_model::repository::Commit {
                id: "c1".into(),
                ..Default::default()
            }])),
            revision: None,
        };
        let opened = scenario()
            .event(work(DetailsWork::default()))
            .send_all([
                shown(Some("a"), Some("/w")),
                shown(None, Some("/w")),
                Msg::DetailsTab(LOG),
            ])
            .event(patch(2).change(DetailsChange::Repository {
                worktree: "/w".into(),
                read: log,
            }))
            .send(Msg::DetailsKey(Key::Open));
        assert_eq!(
            opened.state().details.drill.spot(),
            Some(Spot::Files(Rev::Commit("c1".into())))
        );
        let same = opened.send(shown(Some("a"), Some("/w")));
        assert_eq!(
            same.state().details.drill.spot(),
            Some(Spot::Files(Rev::Commit("c1".into())))
        );
        let moved = same.send(shown(Some("a"), Some("/v")));
        assert_eq!(moved.state().details.drill, Drill::new(LOG));
    }

    #[test]
    fn the_sheet_keys_choose_a_sheet_and_step_along_from_its_top() {
        let cases: Vec<(Vec<Msg>, usize)> = vec![
            (vec![], OVERVIEW),
            (vec![Msg::DetailsTab(CHANGES)], CHANGES),
            (vec![Msg::DetailsTabStep(1)], CHANGES),
            (vec![Msg::DetailsTabStep(-1)], TIMELINE),
            (
                vec![Msg::DetailsTab(TIMELINE), Msg::DetailsTabStep(1)],
                OVERVIEW,
            ),
        ];
        for (msgs, sheet) in cases {
            let run = scenario().send_all(msgs.clone());
            assert_eq!(run.state().details.drill, Drill::new(sheet), "{msgs:?}");
        }
    }

    #[test]
    fn the_keys_on_the_entries_use_the_row_selected() {
        let links = Links {
            urls: vec![link(LinkKind::Url, "https://a.dev")],
            files: vec![link(LinkKind::File(Touch::Edited), "/w/a.rs")],
            ..Default::default()
        };
        // (keys, effects of the last); rows: Links heading, url, Files
        // heading, file.
        let cases: Vec<(Vec<Key>, Vec<Effect>)> = vec![
            (
                vec![Key::Down, Key::Open],
                vec![Effect::Open {
                    environment: here(),
                    target: "https://a.dev".into(),
                }],
            ),
            (
                vec![Key::Down, Key::Copy],
                vec![Effect::Copy {
                    text: "https://a.dev".into(),
                    done: "Copied https://a.dev".into(),
                }],
            ),
            (vec![Key::Down, Key::Reveal], vec![]),
            (
                vec![Key::Last, Key::Reveal],
                vec![Effect::Reveal {
                    environment: here(),
                    path: "/w/a.rs".into(),
                }],
            ),
            (
                vec![Key::Last, Key::Open],
                vec![Effect::Open {
                    environment: here(),
                    target: "/w/a.rs".into(),
                }],
            ),
            (vec![Key::Copy], vec![]),
        ];
        for (keys, want) in cases {
            let run = scenario()
                .event(work(read("a", with_links(links.clone()))))
                .send_all([shown(Some("a"), None), Msg::DetailsTab(RESOURCES)])
                .send_all(keys.iter().copied().map(Msg::DetailsKey));
            assert_eq!(local(&run), want, "{keys:?}");
        }
    }

    #[test]
    fn opening_a_session_row_shows_it_while_it_runs() {
        let peers = |name: &str| Links {
            peers: vec![Peer {
                name: name.into(),
                sent: true,
                received: false,
            }],
            ..Default::default()
        };
        let running = scenario()
            .event(work(read("a", with_links(peers("b")))))
            .send_all([shown(Some("a"), None), Msg::DetailsTab(AGENTS)])
            .send_all([Msg::DetailsKey(Key::Down), Msg::DetailsKey(Key::Open)]);
        assert_eq!(local(&running), [Effect::ShowSession(here(), "b".into())]);
        let gone = scenario()
            .event(work(read("a", with_links(peers("gone")))))
            .send_all([shown(Some("a"), None), Msg::DetailsTab(AGENTS)])
            .send(Msg::DetailsClick(1));
        assert_eq!(local(&gone), [Effect::Notice("gone is not running".into())]);
        let revealed = gone.send(Msg::DetailsKey(Key::Reveal));
        assert_eq!(local(&revealed), []);
    }

    #[test]
    fn a_heading_folds_its_group_and_stays_selected() {
        let links = Links {
            urls: (0..3)
                .map(|i| link(LinkKind::Url, &format!("https://{i}.dev")))
                .collect(),
            files: vec![link(LinkKind::File(Touch::Read), "/w/a.rs")],
            ..Default::default()
        };
        let run = scenario()
            .event(work(read("a", with_links(links))))
            .send_all([shown(Some("a"), None), Msg::DetailsTab(RESOURCES)])
            .send_all([Msg::DetailsKey(Key::Down), Msg::DetailsKey(Key::Fold)]);
        let d = &run.state().details;
        assert!(is_folded(d, Group::Links));
        let listing = entries_of(&run).unwrap();
        assert_eq!(d.drill.selected(&listing), listing.heading(Group::Links));
        let unfolded = run.send(Msg::DetailsKey(Key::Open));
        assert!(!is_folded(&unfolded.state().details, Group::Links));
    }

    #[test]
    fn the_panel_is_open_with_its_side_panel_and_reads_by_the_settings() {
        let run = scenario().event(crate::config::testing::configured(
            "[details]\nlinks_shown = 3\n",
        ));
        assert!(run.state().details.shown.open);
        assert_eq!(run.state().details.shown.settings.links_shown, 3);
        let hidden = run.send(Msg::PanelToggle(crate::layout::Side::Details));
        assert!(!hidden.state().details.shown.open);
        let again = hidden.send(Msg::PanelToggle(crate::layout::Side::Details));
        assert!(again.state().details.shown.open);
    }

    #[test]
    fn a_group_folded_shows_its_first_few_again_once_unfolded() {
        let links = Links {
            files: (0..8)
                .map(|i| link(LinkKind::File(Touch::Read), &format!("/w/{i}.rs")))
                .collect(),
            ..Default::default()
        };
        let run = scenario()
            .event(crate::config::testing::configured(
                "[details]\nlinks_shown = 2\n",
            ))
            .event(work(read("a", with_links(links))))
            .send_all([shown(Some("a"), None), Msg::DetailsTab(RESOURCES)]);
        // Rows: heading, two files, the row to show all.
        let all = run.send(Msg::DetailsClick(3));
        assert_eq!(all.state().details.opened, HashSet::from([Group::Files]));
        let folded = all.send(Msg::DetailsClick(0));
        assert!(folded.state().details.opened.is_empty());
        let unfolded = folded.send(Msg::DetailsClick(0));
        let Some(Listing::Entries(entries)) = entries_of(&unfolded) else {
            panic!("no entries");
        };
        assert_eq!(entries.len(), 4);
    }

    #[test]
    fn process_rows_are_available_without_a_transcript_and_follow_only_the_shown_session() {
        let processes = SessionRead {
            processes: Some(vec![Process {
                key: "process-123".into(),
                pid: 123,
                name: "server".into(),
                ..Default::default()
            }]),
            ..Default::default()
        };
        let run = scenario()
            .event(work(read("session-one", processes)))
            .send(shown(Some("session-one"), None));
        let Some(Listing::Entries(entries)) = entries_of(&run) else {
            panic!("processes without transcript links")
        };
        assert!(
            entries
                .iter()
                .any(|e| matches!(e, Entry::Process(p) if p.pid == 123))
        );
        let other = entries_of(&run.send(shown(Some("session-two"), None)));
        assert!(
            !matches!(other, Some(Listing::Entries(e)) if e.iter().any(|e| matches!(e, Entry::Process(_))))
        );
    }

    #[test]
    fn resources_list_drawn_images_of_the_session_shown_after_its_artifacts() {
        let links = Links {
            artifacts: vec![link(LinkKind::Artifact, "https://claude.ai/artifact/a")],
            urls: vec![link(LinkKind::Url, "https://a.dev")],
            ..Default::default()
        };
        let run = scenario()
            .event(work(read("s1", with_links(links))))
            .send_all([
                shown(Some("s1"), None),
                Msg::DetailsTab(RESOURCES),
                told(Told {
                    drawn: vec![Shot::console(1, 1, 1, 5, None)],
                }),
            ]);
        let Some(Listing::Entries(entries)) = entries_of(&run) else {
            panic!("no entries");
        };
        let groups: Vec<_> = entries
            .iter()
            .filter_map(|e| match e {
                Entry::Heading { group, .. } => Some(*group),
                _ => None,
            })
            .collect();
        assert_eq!(groups, [Group::Artifacts, Group::Images, Group::Links]);
    }

    #[test]
    fn resources_show_the_image_files_they_have_small_copies_of_newest_first() {
        let image = |path: &str, at_ms: u64| adroit_model::links::ImageFile {
            path: path.into(),
            touch: Touch::Read,
            at_ms: Some(at_ms),
        };
        let thumb: Arc<str> = "data:image/png;base64,AA==".into();
        let read_now = SessionRead {
            links: Some(Links {
                images: vec![
                    image("/w/a.png", 1),
                    image("/w/b.png", 3),
                    image("/w/c.png", 4),
                ],
                ..Default::default()
            }),
            thumbs: BTreeMap::from([
                ("/w/a.png".to_string(), Some(thumb.clone())),
                ("/w/b.png".to_string(), None),
            ]),
            ..Default::default()
        };
        let got = shots(
            &[Shot::console(9, 2, 2, 2, None)],
            read_now.links.as_ref().unwrap(),
            Some(&read_now),
        );
        assert_eq!(
            got.iter()
                .map(|s| (s.at_ms, s.thumb.clone()))
                .collect::<Vec<_>>(),
            [(Some(3), None), (Some(2), None), (Some(1), Some(thumb))]
        );
    }

    #[test]
    fn the_console_s_images_and_the_files_run_newest_first_together() {
        let file = |path: &str, at_ms: Option<u64>| adroit_model::links::ImageFile {
            path: path.into(),
            touch: Touch::Read,
            at_ms,
        };
        let thumb: Arc<str> = "data:image/png;base64,AA==".into();
        // A file gone has no small copy, and is not listed.
        let read_now = SessionRead {
            links: Some(Links {
                images: vec![file("/w/a.png", Some(9)), file("/w/b.png", None)],
                ..Default::default()
            }),
            thumbs: BTreeMap::from([("/w/a.png".to_string(), Some(thumb))]),
            ..Default::default()
        };
        let got = shots(
            &[Shot::console(1, 2, 2, 5, None)],
            read_now.links.as_ref().unwrap(),
            Some(&read_now),
        );
        assert_eq!(got.len(), 2);
        assert_eq!(got[0].at_ms, Some(9));
        assert_eq!(got[1].at_ms, Some(5));
    }

    #[test]
    fn resources_list_links_only_for_the_session_they_were_read_for() {
        let links = Links {
            urls: vec![link(LinkKind::Url, "https://a.dev")],
            ..Default::default()
        };
        let resources = |id: Option<&str>| [shown(id, None), Msg::DetailsTab(RESOURCES)];
        let run = scenario().event(work(read("s1", with_links(links))));
        assert!(entries_of(&run.send_all(resources(Some("s1")))).is_some());
        let run = scenario().event(work(read(
            "s1",
            with_links(Links {
                urls: vec![link(LinkKind::Url, "https://a.dev")],
                ..Default::default()
            }),
        )));
        assert!(entries_of(&run.send_all(resources(Some("s2")))).is_none());
        let run = scenario().event(work(read("s1", SessionRead::default())));
        assert!(entries_of(&run.send_all(resources(None))).is_none());
    }

    #[test]
    fn what_is_read_again_steps_back_from_a_file_no_longer_changed() {
        let file = adroit_model::repository::Change {
            status: " M".into(),
            path: "a.rs".into(),
            from: None,
        };
        let files = RepositoryRead {
            spot: Spot::Files(Rev::WorkingCopy),
            listing: Ok(Read::Files(vec![file.clone()])),
            revision: None,
        };
        let diff = |lines: Vec<DiffLine>| DetailsChange::Repository {
            worktree: "/w".into(),
            read: RepositoryRead {
                spot: Spot::Diff(Rev::WorkingCopy, file.clone()),
                listing: Ok(Read::Diff(lines)),
                revision: None,
            },
        };
        let opened = scenario()
            .event(work(DetailsWork::default()))
            .send_all([
                shown(Some("a"), Some("/w")),
                shown(None, Some("/w")),
                Msg::DetailsTab(CHANGES),
            ])
            .event(patch(2).change(DetailsChange::Repository {
                worktree: "/w".into(),
                read: files,
            }))
            .send(Msg::DetailsClick(0));
        let diffing = Some(Spot::Diff(Rev::WorkingCopy, file.clone()));
        assert_eq!(opened.state().details.drill.spot(), diffing);
        let changed = opened.event(patch(3).change(diff(vec![DiffLine {
            kind: DiffKind::Added,
            text: "+x".into(),
        }])));
        assert_eq!(changed.state().details.drill.spot(), diffing);
        let unchanged = changed.event(patch(4).change(diff(vec![])));
        assert_eq!(unchanged.state().details.drill.spot(), None);
    }

    #[test]
    fn a_subagent_gone_from_the_entries_is_gone_back_from() {
        let agent = Subagent {
            call: "toolu_1".into(),
            id: None,
            description: "Look around".into(),
            title: None,
            kind: None,
            prompt: "look".into(),
            standing: Standing::Running,
            started_ms: None,
            ended_ms: None,
            steps: Vec::new(),
        };
        let links = Links {
            agents: vec![agent],
            ..Default::default()
        };
        let opened = scenario()
            .event(work(read("a", with_links(links))))
            .send(shown(Some("a"), None))
            .send_all([Msg::DetailsKey(Key::Down), Msg::DetailsKey(Key::Open)]);
        assert_eq!(opened.state().details.drill.agent(), Some("toolu_1"));
        assert!(
            opened_agent(
                &opened.state().details,
                opened.state().hosts.own().work.as_ref()
            )
            .is_some()
        );
        let scrolled = opened.send(Msg::DetailsKey(Key::Down));
        assert_eq!(local(&scrolled), [Effect::ScrollDetails(Scroll::Down)]);
        let gone = scrolled.event(patch(2).change(DetailsChange::Links {
            session: "a".into(),
            links: Links::default(),
        }));
        assert_eq!(gone.state().details.drill.agent(), None);
    }

    #[test]
    fn the_keys_that_go_back_leave_a_message_opened_for_its_list() {
        let note = Note {
            id: "n1".into(),
            outgoing: false,
            other: "leader".into(),
            body: "Look around".into(),
            sent_ms: 0,
        };
        let opened = scenario()
            .event(work(read(
                "a",
                SessionRead {
                    links: Some(Links::default()),
                    notes: Some(vec![note]),
                    ..Default::default()
                },
            )))
            .send_all([shown(Some("a"), None), Msg::DetailsTab(AGENTS)])
            .send_all([Msg::DetailsKey(Key::Down), Msg::DetailsKey(Key::Open)]);
        assert_eq!(opened.state().details.drill.message(), Some("n1"));
        let scrolled = opened.send(Msg::DetailsKey(Key::Down));
        assert_eq!(local(&scrolled), [Effect::ScrollDetails(Scroll::Down)]);
        let back = scrolled.send(Msg::DetailsKey(Key::Fold));
        assert_eq!(back.state().details.drill.message(), None);
        let Some(ref l @ Listing::Entries(ref entries)) = entries_of(&back) else {
            panic!("the entries are shown");
        };
        let at = back.state().details.drill.selected(l).unwrap();
        assert!(matches!(&entries[at], Entry::Message(n) if n.id == "n1"));
    }

    fn journal(ops: serde_json::Value) -> Vec<adroit_model::timeline::Entry> {
        vec![adroit_model::timeline::Entry {
            at_ms: 1,
            ops: serde_json::from_value(ops).unwrap(),
        }]
    }

    /// A tree `goal` whose task `a` is done, `b` in progress and `c` to do.
    fn working() -> SessionRead {
        SessionRead {
            links: Some(Links::default()),
            timeline: Some(journal(serde_json::json!([
                {"op": "add", "id": "goal", "title": "Goal"},
                {"op": "add", "id": "a", "parent": "goal", "title": "A", "status": "completed"},
                {"op": "add", "id": "b", "parent": "goal", "title": "B", "status": "in_progress"},
                {"op": "add", "id": "c", "parent": "goal", "title": "C"},
            ]))),
            ..Default::default()
        }
    }

    fn selected_key(run: &Scenario) -> Option<String> {
        let Some(ref l @ Listing::Entries(ref entries)) = entries_of(run) else {
            return None;
        };
        let at = run.state().details.drill.selected(l)?;
        Some(match &entries[at] {
            Entry::Tree(card) => format!("tree {}", card.id),
            Entry::Task(line) => line.id.clone(),
            other => format!("{other:?}"),
        })
    }

    fn timeline_rows(run: &Scenario) -> usize {
        match entries_of(run) {
            Some(Listing::Entries(entries)) => entries.len(),
            _ => 0,
        }
    }

    #[test]
    fn a_task_of_the_timeline_opens_its_history_and_the_keys_that_go_back_leave_it_for_its_tree() {
        let shown_tree = scenario()
            .event(work(read("a", working())))
            .send_all([shown(Some("a"), None), Msg::DetailsTab(TIMELINE)]);
        assert_eq!(timeline_rows(&shown_tree), 4);
        let opened = shown_tree.send_all([
            Msg::DetailsKey(Key::Down),
            Msg::DetailsKey(Key::Down),
            Msg::DetailsKey(Key::Open),
        ]);
        assert_eq!(opened.state().details.drill.task(), Some("b"));
        let detail = opened_task(
            &opened.state().details,
            opened.state().hosts.own().work.as_ref(),
        )
        .unwrap();
        assert_eq!(
            (detail.tree.as_str(), detail.task.title.as_str()),
            ("Goal", "B")
        );
        let back = opened.send(Msg::DetailsKey(Key::Fold));
        assert_eq!(back.state().details.drill.task(), None);
        assert_eq!(selected_key(&back).as_deref(), Some("b"));
    }

    #[test]
    fn a_tree_folds_from_its_card_and_from_its_tasks() {
        let shown_tree = scenario()
            .event(work(read("a", working())))
            .send_all([shown(Some("a"), None), Msg::DetailsTab(TIMELINE)]);
        let folded = shown_tree.send(Msg::DetailsKey(Key::Open));
        assert_eq!(timeline_rows(&folded), 1);
        let unfolded = folded.send(Msg::DetailsKey(Key::Open));
        assert_eq!(timeline_rows(&unfolded), 4);
        let from_task = unfolded.send_all([Msg::DetailsKey(Key::Last), Msg::DetailsKey(Key::Fold)]);
        assert_eq!(timeline_rows(&from_task), 1);
        assert_eq!(selected_key(&from_task).as_deref(), Some("tree goal"));
        let clicked = from_task.send(Msg::DetailsClick(0));
        assert_eq!(timeline_rows(&clicked), 4);
    }

    /// Trees `t1` to `t4`, each with a task to do, the newest last.
    fn planned() -> SessionRead {
        let ops: Vec<serde_json::Value> = (1..=4)
            .flat_map(|n| {
                [
                    serde_json::json!({"op": "add", "id": format!("t{n}"), "title": format!("T{n}")}),
                    serde_json::json!({"op": "add", "id": format!("t{n}a"), "parent": format!("t{n}"), "title": "A"}),
                ]
            })
            .collect();
        SessionRead {
            links: Some(Links::default()),
            timeline: Some(journal(serde_json::Value::Array(ops))),
            ..Default::default()
        }
    }

    fn timeline_keys(run: &Scenario) -> Vec<String> {
        let Some(Listing::Entries(entries)) = entries_of(run) else {
            return Vec::new();
        };
        entries
            .iter()
            .filter_map(|e| match e {
                Entry::Tree(card) => Some(format!("tree {}", card.id)),
                Entry::Task(line) => Some(line.id.clone()),
                Entry::More {
                    group: Group::Timeline,
                    hidden,
                } => Some(format!("more {hidden}")),
                _ => None,
            })
            .collect()
    }

    #[test]
    fn the_overview_lists_the_newest_trees_each_folding_on_its_own() {
        let overview = scenario()
            .event(work(read("a", planned())))
            .send(shown(Some("a"), None));
        assert_eq!(
            timeline_keys(&overview),
            [
                "tree t4", "t4a", "tree t3", "t3a", "tree t2", "t2a", "more 1"
            ]
        );
        assert_eq!(selected_key(&overview).as_deref(), Some("tree t4"));
        let folded = overview.send(Msg::DetailsKey(Key::Open));
        assert_eq!(
            timeline_keys(&folded),
            ["tree t4", "tree t3", "t3a", "tree t2", "t2a", "more 1"]
        );
        assert_eq!(selected_key(&folded).as_deref(), Some("tree t4"));
        let from_task = folded.send_all([
            Msg::DetailsKey(Key::Down),
            Msg::DetailsKey(Key::Down),
            Msg::DetailsKey(Key::Fold),
        ]);
        assert_eq!(
            timeline_keys(&from_task),
            ["tree t4", "tree t3", "tree t2", "t2a", "more 1"]
        );
        assert_eq!(selected_key(&from_task).as_deref(), Some("tree t3"));
    }

    #[test]
    fn the_overview_s_row_past_its_trees_shows_them_all() {
        let overview = scenario()
            .event(work(read("a", planned())))
            .send(shown(Some("a"), None));
        let at = {
            let Some(Listing::Entries(entries)) = entries_of(&overview) else {
                panic!("the overview lists its rows");
            };
            entries
                .iter()
                .position(|e| {
                    matches!(
                        e,
                        Entry::More {
                            group: Group::Timeline,
                            ..
                        }
                    )
                })
                .unwrap()
        };
        let all = overview.send(Msg::DetailsClick(at));
        assert_eq!(
            timeline_keys(&all),
            [
                "tree t4", "t4a", "tree t3", "t3a", "tree t2", "t2a", "tree t1", "t1a"
            ]
        );
    }

    #[test]
    fn the_overview_says_nothing_of_a_timeline_its_agent_never_recorded() {
        let overview = scenario()
            .event(work(read(
                "a",
                SessionRead {
                    links: Some(Links::default()),
                    ..Default::default()
                },
            )))
            .send(shown(Some("a"), None));
        assert_eq!(timeline_keys(&overview), Vec::<String>::new());
    }

    #[test]
    fn a_task_opened_that_is_removed_meanwhile_is_gone_back_from() {
        let opened = scenario()
            .event(work(read("a", working())))
            .send_all([shown(Some("a"), None), Msg::DetailsTab(TIMELINE)])
            .send_all([Msg::DetailsKey(Key::Last), Msg::DetailsKey(Key::Open)]);
        assert_eq!(opened.state().details.drill.task(), Some("c"));
        let mut entries = working().timeline.unwrap();
        entries.push(adroit_model::timeline::Entry {
            at_ms: 2,
            ops: serde_json::from_value(serde_json::json!([{"op": "remove", "id": "c"}])).unwrap(),
        });
        let gone = opened.event(patch(2).change(DetailsChange::Timeline {
            session: "a".into(),
            entries,
        }));
        assert_eq!(gone.state().details.drill.task(), None);
    }

    #[test]
    fn a_sheet_without_a_list_scrolls() {
        let cases = [
            (Key::Down, Scroll::Down),
            (Key::Up, Scroll::Up),
            (Key::First, Scroll::Top),
            (Key::Last, Scroll::Bottom),
        ];
        for (key, scroll) in cases {
            let run = scenario().send(Msg::DetailsKey(key));
            assert_eq!(local(&run), [Effect::ScrollDetails(scroll)], "{key:?}");
        }
    }

    #[test]
    fn the_foot_shows_the_context_asked_for_only_while_its_conversation_goes_on() {
        let conversation = |id: &str| ConversationRead {
            id: id.into(),
            stats: ConversationStats::default(),
        };
        let asked = |id: &str| ContextRead {
            conversation: id.into(),
            context: (50_000, 200_000),
        };
        let limits = vec![Limit {
            window: Window::Session,
            percent: 10.0,
            resets_ms: None,
            severity: Severity::Normal,
        }];
        // (conversation read, context asked of, usage shown, gauges, the
        // first a context's)
        let cases = [
            (
                "a-conversation",
                "a-conversation",
                Some(Agent::Claude),
                2,
                true,
            ),
            ("a-conversation", "old", Some(Agent::Claude), 1, false),
            ("old", "a-conversation", Some(Agent::Claude), 1, false),
            ("a-conversation", "a-conversation", None, 1, true),
        ];
        for (read_id, asked_id, usage_of, count, context) in cases {
            let mut details = read(
                "a",
                SessionRead {
                    conversation: Some(conversation(read_id)),
                    context: Some(asked(asked_id)),
                    ..Default::default()
                },
            );
            details.usage.insert(
                Account {
                    agent: Agent::Claude,
                    home: None,
                },
                limits.clone(),
            );
            let run = scenario()
                .event(
                    work(details)
                        .with(|w| w.agents = with_usage(Agent::Claude, usage_of.is_some())),
                )
                .send(shown(Some("a"), None));
            let got = usage(
                &run.state().details,
                run.state().hosts.own().work.as_ref(),
                0,
            );
            assert_eq!(
                (got.len(), got[0].context()),
                (count, context),
                "{read_id} {asked_id} {usage_of:?}"
            );
        }
    }

    #[test]
    fn usage_depends_only_on_the_console_agent() {
        for agent in [Agent::Claude, Agent::Codex] {
            let facts = with_usage(agent, true);
            let facts = facts.of(agent);
            let account = Account { agent, home: None };
            let usage = |tab: Option<&Tab>| {
                let mut details = Details::default();
                follow_usage(&mut details, tab, account.clone(), facts);
                details.shown.usage
            };
            for tab in [
                Tab::Session(here(), "session".into()),
                Tab::Draft {
                    environment: here(),
                    worktree: "/project".into(),
                    id: Default::default(),
                },
            ] {
                assert_eq!(usage(Some(&tab)), Some(account.clone()));
            }
            assert_eq!(usage(Some(&Tab::Terminal(here(), 1))), None);
            assert_eq!(usage(None), None);
        }
        let mut details = Details::default();
        let draft = Tab::draft(here(), "/project");
        let codex = Account {
            agent: Agent::Codex,
            home: None,
        };
        follow_usage(&mut details, Some(&draft), codex, None);
        assert_eq!(details.shown.usage, None, "nothing told of the agent");
    }

    #[test]
    fn about_is_folded_until_unfolded_and_copies_the_fact_selected() {
        let run = scenario()
            .event(work(read("a", SessionRead::default())))
            .send(shown(Some("a"), Some("/w")));
        let Some(Listing::Entries(entries)) = entries_of(&run) else {
            panic!("no entries");
        };
        assert!(matches!(
            entries.as_slice(),
            [Entry::Heading {
                group: Group::About,
                folded: true,
                ..
            }]
        ));
        let unfolded = run.send(Msg::DetailsKey(Key::Open));
        let Some(Listing::Entries(entries)) = entries_of(&unfolded) else {
            panic!("no entries");
        };
        let worktree = entries
            .iter()
            .position(|e| matches!(e, Entry::Fact(f) if f.label == "Worktree"))
            .expect("the worktree is told");
        let copied = unfolded
            .send(Msg::DetailsSelect(worktree))
            .send(Msg::DetailsKey(Key::Copy));
        assert_eq!(
            local(&copied),
            [Effect::Copy {
                text: "/w".into(),
                done: "Copied /w".into(),
            }]
        );
        let opened = copied.send(Msg::DetailsKey(Key::Open));
        assert_eq!(local(&opened), []);
    }

    #[test]
    fn a_row_under_running_folds_running() {
        let processes = SessionRead {
            processes: Some(vec![Process {
                key: "process-1".into(),
                pid: 1,
                name: "server".into(),
                ..Default::default()
            }]),
            ..Default::default()
        };
        let run = scenario()
            .event(work(read("a", processes)))
            .send(shown(Some("a"), None))
            .send_all([Msg::DetailsKey(Key::Down), Msg::DetailsKey(Key::Fold)]);
        assert!(is_folded(&run.state().details, Group::Running));
        let listing = entries_of(&run).unwrap();
        assert_eq!(
            run.state().details.drill.selected(&listing),
            listing.heading(Group::Running)
        );
    }

    #[test]
    fn the_overview_tells_the_change_the_worktree_is_on_from_its_files_read() {
        let files = RepositoryRead {
            spot: Spot::Files(Rev::WorkingCopy),
            listing: Ok(Read::Files(Vec::new())),
            revision: Some(adroit_model::repository::Revision {
                id: "x-full".into(),
                short: "x".into(),
                description: "core: keep it\n\nWhy.".into(),
            }),
        };
        let run = scenario()
            .event(work(DetailsWork::default()))
            .send(shown(Some("a"), Some("/w")))
            .event(patch(2).change(DetailsChange::Repository {
                worktree: "/w".into(),
                read: files,
            }));
        let change = working_copy(&run.state().details, run.state().hosts.own().work.as_ref());
        assert_eq!(
            change.and_then(|v| v.revision).map(|r| r.short),
            Some("x".into())
        );
        let log = run.send(Msg::DetailsTab(LOG));
        assert_eq!(
            watched(&log).last().unwrap().repositories[0].spot,
            Spot::Log
        );
    }

    #[test]
    fn the_agents_and_resources_sheets_are_read_from_the_conversation_and_not_the_worktree() {
        for sheet in [AGENTS, RESOURCES] {
            let run = scenario().send_all([shown(Some("a"), Some("/w")), Msg::DetailsTab(sheet)]);
            let sight = watched(&run).last().cloned().unwrap();
            assert!(sight.overview, "{sheet:?}");
            assert!(sight.repositories.is_empty(), "{sheet:?}");
        }
    }

    /// Resources of `a` listing an image its console drew and, older, an
    /// image file: rows Images heading, the drawn one, the file.
    pub(crate) fn with_images() -> Scenario {
        let read_now = SessionRead {
            links: Some(Links {
                images: vec![adroit_model::links::ImageFile {
                    path: "/w/plot.png".into(),
                    touch: Touch::Edited,
                    at_ms: Some(1),
                }],
                ..Default::default()
            }),
            thumbs: BTreeMap::from([("/w/plot.png".to_string(), None)]),
            ..Default::default()
        };
        scenario().event(work(read("a", read_now))).send_all([
            shown(Some("a"), Some("/w")),
            Msg::DetailsTab(RESOURCES),
            told(Told {
                drawn: vec![Shot::console(7, 64, 32, 5, None)],
            }),
        ])
    }

    pub(crate) fn drawn_tab() -> Tab {
        Tab::Drawn {
            environment: here(),
            session: "a".into(),
            id: 7,
            width: 64,
            height: 32,
        }
    }

    #[test]
    fn opening_an_image_looks_at_it_in_the_lightbox() {
        for (keys, thing) in [
            (vec![Key::Down, Key::Open], drawn_tab()),
            (
                vec![Key::Last, Key::Open],
                Tab::Viewer(here(), "/w/plot.png".into()),
            ),
        ] {
            let run = with_images().send_all(keys.iter().copied().map(Msg::DetailsKey));
            assert_eq!(local(&run), [Effect::Lightbox(thing)], "{keys:?}");
        }
    }

    #[test]
    fn opening_an_image_in_a_split_or_a_tab_shows_it_in_the_content() {
        let (drawn, file) = (drawn_tab(), Tab::Viewer(here(), "/w/plot.png".into()));
        // (keys, what is shown, how)
        let cases = [
            (
                vec![Key::Down, Key::OpenIn(Opening::Split(Axis::Row))],
                &drawn,
                Opening::Split(Axis::Row),
            ),
            (
                vec![Key::Last, Key::OpenIn(Opening::Tab)],
                &file,
                Opening::Tab,
            ),
            (
                vec![Key::Last, Key::OpenIn(Opening::Split(Axis::Column))],
                &file,
                Opening::Split(Axis::Column),
            ),
        ];
        for (keys, thing, how) in cases {
            let run = with_images().send_all(keys.iter().copied().map(Msg::DetailsKey));
            assert_eq!(
                local(&run),
                [Effect::Show {
                    thing: thing.clone(),
                    how
                }],
                "{keys:?}"
            );
        }
    }

    #[test]
    fn a_click_on_an_image_looks_at_it_in_the_lightbox() {
        let (drawn, file) = (drawn_tab(), Tab::Viewer(here(), "/w/plot.png".into()));
        for (at, thing) in [(1, drawn), (2, file)] {
            let run = with_images().send(Msg::DetailsClick(at));
            assert_eq!(local(&run), [Effect::Lightbox(thing.clone())], "{thing:?}");
        }
    }

    #[test]
    fn the_lightbox_holds_the_images_of_resources_in_their_order() {
        let file = Tab::Viewer(here(), "/w/plot.png".into());
        let run = with_images();
        let work = run.state().hosts.work(&here());
        assert_eq!(
            images(&run.state().details, work, true),
            [drawn_tab(), file]
        );
        // A work machine's files are read on this computer by no viewer.
        assert_eq!(images(&run.state().details, work, false), [drawn_tab()]);
    }

    #[test]
    fn only_an_image_opens_in_a_split_or_a_tab() {
        let links = Links {
            urls: vec![link(LinkKind::Url, "https://a.dev")],
            ..Default::default()
        };
        // (key on the address, effects)
        let cases = [
            (Key::OpenIn(Opening::Split(Axis::Row)), vec![]),
            (Key::OpenIn(Opening::Tab), vec![]),
        ];
        for (key, want) in cases {
            let run = scenario()
                .event(work(read("a", with_links(links.clone()))))
                .send_all([shown(Some("a"), None), Msg::DetailsTab(RESOURCES)])
                .send_all([Msg::DetailsKey(Key::Down), Msg::DetailsKey(key)]);
            assert_eq!(local(&run), want, "{key:?}");
        }
    }

    #[test]
    fn the_panel_stays_on_the_session_while_one_of_its_images_is_shown() {
        for thing in [drawn_tab(), Tab::Viewer(here(), "/w/plot.png".into())] {
            let run = with_images().send(Msg::Show {
                thing: thing.clone(),
                place: None,
                how: Some(Opening::Existing),
            });
            assert_eq!(
                run.state().details.shown.session.as_deref(),
                Some("a"),
                "{thing:?}"
            );
            assert!(entries_of(&run).is_some(), "{thing:?}");
        }
    }

    #[test]
    fn a_file_shown_in_another_worktree_describes_no_session() {
        let run = with_images().send(Msg::Show {
            thing: Tab::Viewer(here(), "/elsewhere/a.png".into()),
            place: Some(Place::at(&here(), "/elsewhere")),
            how: None,
        });
        assert_eq!(run.state().details.shown.session, None);
    }
    /// A file and an address the session `a` came across.
    fn file_and_url() -> SessionRead {
        with_links(Links {
            urls: vec![link(LinkKind::Url, "https://a.dev")],
            files: vec![link(LinkKind::File(Touch::Read), "/w/a.rs")],
            ..Default::default()
        })
    }

    /// The session `a` shown with the panel made of the settings `panel`.
    fn laid_out(panel: &str) -> Scenario {
        Scenario::new()
            .event(crate::config::testing::configured(panel))
            .event(work(read("a", file_and_url())))
            .send(shown(Some("a"), Some("/w")))
    }

    fn keys(listing: Option<Listing>) -> Vec<String> {
        match listing {
            Some(Listing::Entries(entries)) => entries
                .iter()
                .map(|e| match e {
                    Entry::Heading { group, folded, .. } => {
                        format!(
                            "{}{}",
                            group.title(),
                            if *folded { " (folded)" } else { "" }
                        )
                    }
                    Entry::Link(l) => l.target.clone(),
                    other => format!("{other:?}"),
                })
                .collect(),
            _ => Vec::new(),
        }
    }

    #[test]
    fn a_tab_of_the_user_s_lists_the_rows_of_its_components_in_their_order() {
        let run = laid_out(
            "[[panel.session.tabs]]\ntitle = \"Mine\"\ncomponents = [\"links\", \"files\"]\n",
        );
        assert_eq!(
            keys(entries_of(&run)),
            ["Links", "https://a.dev", "Files", "/w/a.rs"]
        );
    }

    #[test]
    fn a_component_alone_in_its_tab_lists_its_rows_without_a_heading() {
        let run = laid_out(
            "[[panel.session.tabs]]\ntitle = \"Files\"\ncomponents = [\"files\"]\n\
             [[panel.session.tabs]]\ntitle = \"Both\"\ncomponents = [\"files\", \"links\"]\n",
        );
        assert_eq!(keys(entries_of(&run)), ["/w/a.rs"]);
        let both = run.send(Msg::DetailsTab(1));
        assert_eq!(keys(entries_of(&both))[0], "Files");
    }

    #[test]
    fn a_component_placed_folded_starts_folded_until_its_heading_is_opened() {
        let run = laid_out(
            "[[panel.session.tabs]]\ntitle = \"T\"\n\
             components = [\"files\", { component = \"links\", folded = true }]\n",
        );
        assert_eq!(
            keys(entries_of(&run)),
            ["Files", "/w/a.rs", "Links (folded)"]
        );
        let opened = run.send(Msg::DetailsClick(2));
        assert_eq!(
            keys(entries_of(&opened)),
            ["Files", "/w/a.rs", "Links", "https://a.dev"]
        );
        let shut = opened.send(Msg::DetailsClick(2));
        assert!(shut.state().details.toggled.is_empty());
    }

    #[test]
    fn a_component_placed_to_show_fewer_shows_its_first_and_a_row_for_the_rest() {
        let links = Links {
            files: (0..4)
                .map(|i| link(LinkKind::File(Touch::Read), &format!("/w/{i}.rs")))
                .collect(),
            ..Default::default()
        };
        let run = scenario()
            .event(crate::config::testing::configured(
                "[[panel.session.tabs]]\ntitle = \"T\"\n\
                 components = [\"links\", { component = \"files\", shown = 1 }]\n",
            ))
            .event(work(read("a", with_links(links))))
            .send(shown(Some("a"), None));
        let Some(Listing::Entries(entries)) = entries_of(&run) else {
            panic!("rows listed");
        };
        assert!(matches!(
            entries.as_slice(),
            [
                Entry::Heading { .. },
                Entry::Link(_),
                Entry::More { hidden: 3, .. }
            ]
        ));
    }

    #[test]
    fn the_host_reads_only_what_the_components_in_sight_need() {
        let sight = |panel: &str| {
            watched(&laid_out(panel))
                .last()
                .cloned()
                .expect("the host is asked")
        };
        let machine = sight(
            "[panel.session]\nfoot = []\n[[panel.session.tabs]]\ntitle = \"M\"\ncomponents = [\"cpu\", \"gpu\", \"memory\", \"storage\"]\n",
        );
        assert!(!machine.overview);
        assert!(machine.repositories.is_empty());
        assert_eq!(machine.usage, None);
        assert_eq!(machine.system, Some(UsageSettings::default()));
        let repository = sight(
            "[panel.session]\nfoot = []\n[[panel.session.tabs]]\ntitle = \"R\"\ncomponents = [\"log\", \"changes\"]\n",
        );
        let spots: Vec<Spot> = repository
            .repositories
            .into_iter()
            .map(|r| r.spot)
            .collect();
        assert_eq!(spots, [Spot::Files(Rev::WorkingCopy), Spot::Log]);
        assert_eq!(repository.system, Some(UsageSettings::NONE));
        let links = sight(
            "[panel.session]\nfoot = []\n[[panel.session.tabs]]\ntitle = \"L\"\ncomponents = [\"links\"]\n",
        );
        assert!(links.overview);
    }

    #[test]
    fn parts_beside_each_other_share_a_line_and_lines_of_meters_or_of_gauges_are_drawn_together() {
        let part = |c, foot, beside| Part {
            placed: Placed {
                beside,
                ..Placed::new(c)
            },
            place: 0,
            at: 0,
            rows: Vec::new(),
            foot,
            said: None,
        };
        use Component as C;
        let runs = runs(vec![
            part(C::Cpu, false, false),
            part(C::Gpu, false, true),
            part(C::Memory, false, false),
            part(C::Session, false, false),
            part(C::Files, false, true),
            part(C::Storage, false, false),
            part(C::Gpu, true, true),
            part(C::Context, true, false),
            part(C::WeekLimit, true, false),
            part(C::Cpu, true, false),
        ]);
        let names: Vec<Vec<Vec<&str>>> = runs
            .iter()
            .map(|r| {
                r.iter()
                    .map(|l| l.iter().map(|p| p.placed.component.name()).collect())
                    .collect()
            })
            .collect();
        assert_eq!(
            names,
            [
                vec![vec!["cpu", "gpu"], vec!["memory"]],
                vec![vec!["session", "files"]],
                vec![vec!["storage"]],
                vec![vec!["gpu"]],
                vec![vec!["context"], vec!["week-limit"]],
                vec![vec!["cpu"]],
            ]
        );
    }

    #[test]
    fn the_account_s_usage_is_asked_only_while_one_of_its_limits_is_in_sight() {
        let mut details = Details::default();
        details.shown.usage = Some(adroit_protocol::Account {
            agent: adroit_model::Agent::Claude,
            home: None,
        });
        let mut layout = DetailsLayout::default();
        let mut only = |components: &[Component]| {
            let mut s = Surface::session();
            s.foot = components.iter().copied().map(Placed::new).collect();
            layout.set(Subject::Session, s);
            let settings = details.shown.settings.clone();
            follow_settings(&mut details, true, &settings, &layout);
            sight(&details).usage.is_some()
        };
        assert!(!only(&[Component::Context, Component::Cpu]));
        assert!(only(&[Component::WeekLimit]));
    }

    #[test]
    fn the_machine_s_meters_sampled_are_those_its_components_in_sight_name_together() {
        let run = laid_out(
            "[panel.session]\n\
             foot = [\"memory\"]\n\
             [[panel.session.tabs]]\ntitle = \"M\"\n\
             components = [\"cpu\"]\n\
             [[panel.session.tabs]]\ntitle = \"N\"\n\
             components = [\"storage\"]\n",
        );
        let system = watched(&run).last().and_then(|s| s.system);
        assert_eq!(
            system,
            Some(UsageSettings {
                cpu: true,
                memory: true,
                ..UsageSettings::NONE
            })
        );
    }

    #[test]
    fn showing_a_component_shows_the_tab_shown_when_it_holds_it_or_else_the_first_that_does() {
        let run = scenario().send(Msg::DetailsShowComponent(Component::Changes));
        assert_eq!(run.state().details.drill.tab(), CHANGES);
        let laid = laid_out(
            "[[panel.session.tabs]]\ntitle = \"A\"\ncomponents = [\"files\"]\n\
             [[panel.session.tabs]]\ntitle = \"B\"\ncomponents = [\"changes\"]\n\
             [[panel.session.tabs]]\ntitle = \"C\"\ncomponents = [\"changes\", \"log\"]\n",
        );
        let first = laid.send(Msg::DetailsShowComponent(Component::Changes));
        assert_eq!(first.state().details.drill.tab(), 1);
        let kept = first
            .send(Msg::DetailsTab(2))
            .send(Msg::DetailsShowComponent(Component::Changes));
        assert_eq!(kept.state().details.drill.tab(), 2);
        let none = kept.send(Msg::DetailsShowComponent(Component::Timeline));
        assert_eq!(none.state().details.drill.tab(), 2);
    }

    #[test]
    fn a_tab_the_layout_does_not_have_is_not_shown_and_one_it_no_longer_has_gives_way_to_its_last()
    {
        let run = scenario().send(Msg::DetailsTab(TIMELINE));
        assert_eq!(run.state().details.drill.tab(), TIMELINE);
        assert_eq!(
            run.send(Msg::DetailsTab(8)).state().details.drill.tab(),
            TIMELINE
        );
        let mut details = Details::default();
        let last = Surface::session().tabs.len() - 1;
        show_tab(&mut details, last);
        let mut layout = DetailsLayout::default();
        let mut two = Surface::session();
        two.tabs.truncate(2);
        layout.set(Subject::Session, two);
        let settings = details.shown.settings.clone();
        follow_settings(&mut details, true, &settings, &layout);
        assert_eq!(details.drill.tab(), 1);
    }

    #[test]
    fn each_thing_the_panel_describes_comes_back_to_the_tab_it_was_left_on() {
        let mut details = Details::default();
        show_tab(&mut details, AGENTS);
        follow_selected(&mut details, Some(&Item::Environment(here())));
        assert_eq!(details.shown.subject(), Subject::Environment);
        assert_eq!(details.drill.tab(), 0);
        follow_selected(&mut details, Some(&Item::Session(here(), "a".into())));
        assert_eq!(details.shown.subject(), Subject::Session);
        assert_eq!(details.drill.tab(), AGENTS);
    }
    #[test]
    fn the_pull_request_of_the_branch_shown_is_the_one_open_for_it() {
        let pull = |number: u64, head: &str| adroit_model::github::Item {
            number,
            pull: Some(adroit_model::github::Pull {
                head: head.into(),
                base: "trunk".into(),
                additions: 0,
                deletions: 0,
                changed_files: 0,
                review: None,
                requested: Vec::new(),
                checks: Default::default(),
            }),
            ..Default::default()
        };
        let mut repository = adroit_model::github::Repository::new("o/r");
        repository.pulls = Some(Ok(vec![pull(3, "other"), pull(7, "main")]));
        let run = scenario()
            .event(snapshot().listed(listed().session(session("a").cwd("/r"))))
            .event(patch(2).change(adroit_protocol::TreeChange::GitHub {
                repository: "/r".into(),
                github: Some(Box::new(repository)),
            }))
            .send(shown(Some("a"), Some("/r")));
        let work = run.state().hosts.own().work.as_ref();
        assert_eq!(
            branch_pull(&run.state().details, work),
            Some(("/r".into(), 7))
        );
        let elsewhere = run.send(shown(Some("a"), Some("/v")));
        let work = elsewhere.state().hosts.own().work.as_ref();
        assert_eq!(branch_pull(&elsewhere.state().details, work), None);
    }
    #[test]
    fn the_pull_requests_of_the_repository_shown_are_read_while_github_is_in_sight() {
        let reads = |panel: &str| {
            let run = scenario()
                .event(crate::config::testing::configured(panel))
                .event(snapshot().listed(listed().session(session("a").cwd("/r"))))
                .send(shown(Some("a"), Some("/r")));
            watched(&run).last().map(|s| s.tree.github.clone())
        };
        let pulls = std::collections::BTreeSet::from([(
            PathBuf::from("/r"),
            adroit_model::github::Listing::Pulls,
        )]);
        assert_eq!(
            reads("[[panel.session.tabs]]\ntitle = \"PR\"\ncomponents = [\"github\"]\n"),
            Some(pulls.clone())
        );
        assert_eq!(
            reads("[[panel.session.tabs]]\ntitle = \"C\"\ncomponents = [\"github-checks\"]\n"),
            Some(pulls.clone()),
            "any component of the branch's pull request reads them"
        );
        assert_eq!(
            reads("[[panel.session.tabs]]\ntitle = \"F\"\ncomponents = [\"files\"]\n"),
            Some(Default::default())
        );
    }
    #[test]
    fn the_prompts_sent_open_whole_and_copy_their_text() {
        let prompt = |turn: u64, text: &str| adroit_model::links::Prompt {
            turn,
            text: text.into(),
            at_ms: None,
        };
        let links = Links {
            prompts: vec![
                prompt(2, "add a test\nfor the login"),
                prompt(1, "fix the login"),
            ],
            ..Default::default()
        };
        let run = scenario()
            .event(crate::config::testing::configured(
                "[[panel.session.tabs]]\ntitle = \"P\"\ncomponents = [\"prompts\"]\n",
            ))
            .event(work(read("a", with_links(links))))
            .send(shown(Some("a"), None));
        let Some(Listing::Entries(entries)) = entries_of(&run) else {
            panic!("rows listed");
        };
        let summaries: Vec<&str> = entries
            .iter()
            .filter_map(|e| match e {
                Entry::Prompt(p) => Some(p.summary()),
                _ => None,
            })
            .collect();
        assert_eq!(summaries, ["add a test", "fix the login"]);
        let opened = run.send(Msg::DetailsKey(Key::Open));
        let work = opened.state().hosts.own().work.as_ref();
        assert_eq!(
            opened_prompt(&opened.state().details, work).map(|p| p.turn),
            Some(2)
        );
        let back = opened
            .send(Msg::DetailsBack)
            .send(Msg::DetailsKey(Key::Down));
        let copied = back.send(Msg::DetailsKey(Key::Copy));
        assert!(copied.effects().iter().any(|e| matches!(
            e,
            Effect::Copy { text, .. } if text == "fix the login"
        )));
    }
}
