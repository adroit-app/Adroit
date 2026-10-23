//! The details panel as the window holds it: how far into which sheet it has
//! gone, the overview's groups folded or shown whole, what the window shows
//! that the panel reads by, and what the panel asks the work host to read.

use std::collections::HashSet;
use std::path::PathBuf;

use adroit_model::links::{Kind as LinkKind, Link, Links};
use adroit_model::repository::Spot;
use adroit_model::{Agent, ConversationStats, Note, Process, Session};
use adroit_protocol::{DetailsWork, RepositorySight, Seq, SessionRead, Sight, WorkSnapshot};

use super::{
    Drill, Entry, Gauge, Group, Listing, Related, Sheet, Shot, Source, View, capped, context_gauge,
    gauges, newest_first, overview_shown, related, with_agents, with_images, with_notes,
    with_processes,
};
use crate::effect::Effect;
use crate::settings::DetailsSettings;
use crate::tabs::Tab;
use crate::tree::Showing;

/// What the window shows that the details panel reads by.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Shown {
    /// The session shown in the middle, by its id.
    pub session: Option<String>,
    /// The worktree shown.
    pub worktree: Option<PathBuf>,
    /// Whether the details panel is open.
    pub open: bool,
    /// The agent whose account usage the panel shows, when what is shown
    /// has one.
    pub usage: Option<Agent>,
    /// The images the shown session's console drew.
    pub drawn: Vec<Shot>,
    pub settings: DetailsSettings,
}

/// What the window tells the details panel of what it shows, beyond the
/// session and the worktree the tree follows.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Told {
    pub open: bool,
    pub usage: Option<Agent>,
    pub drawn: Vec<Shot>,
    pub settings: DetailsSettings,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Details {
    pub drill: Drill,
    /// The overview's groups shown by their headings alone.
    pub folded: HashSet<Group>,
    /// The overview's groups shown whole, past their first few.
    pub opened: HashSet<Group>,
    pub shown: Shown,
    /// The work and the spot the selection was last brought in line with.
    refreshed: Option<(Seq, Option<Spot>)>,
}

impl Default for Details {
    fn default() -> Self {
        Self {
            drill: Drill::new(Sheet::Overview),
            folded: HashSet::new(),
            opened: HashSet::new(),
            shown: Shown::default(),
            refreshed: None,
        }
    }
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
    Fold,
    Copy,
    Reveal,
}

/// How the panel scrolls what is not a list.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Scroll {
    Down,
    Up,
    Top,
    Bottom,
}

/// What is done with a row of the overview.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Use {
    Open,
    Copy,
    Reveal,
}

/// The window told the panel `told`.
pub(crate) fn tell(details: &mut Details, told: Told) -> Vec<Effect> {
    let Told {
        open,
        usage,
        drawn,
        settings,
    } = told;
    details.shown = Shown {
        open,
        usage,
        drawn,
        settings,
        ..std::mem::take(&mut details.shown)
    };
    Vec::new()
}

/// The panel reads by what the focused window shows. What was opened in
/// one worktree means nothing in another, so a new worktree takes the panel
/// back to its sheet's top.
pub(crate) fn follow(details: &mut Details, showing: &Showing) {
    let session = match &showing.tab {
        Some(Tab::Session(id)) => Some(id.clone()),
        _ => None,
    };
    let worktree = showing.place.as_ref().map(|p| p.worktree.clone());
    if worktree != details.shown.worktree {
        details.drill = Drill::new(details.drill.sheet());
        details.shown.worktree = worktree;
    }
    if session != details.shown.session {
        details.shown.session = session;
    }
}

/// `sheet` chosen; one already shown stays where it is.
pub(crate) fn sheet(details: &mut Details, sheet: Sheet) -> Vec<Effect> {
    if details.drill.sheet() != sheet {
        details.drill = Drill::new(sheet);
    }
    Vec::new()
}

/// The sheet `delta` along from the one shown, from its top.
pub(crate) fn step_sheet(details: &mut Details, delta: isize) -> Vec<Effect> {
    details.drill = Drill::new(details.drill.sheet().step(delta));
    Vec::new()
}

/// Back out to the list the shown row was opened from.
pub(crate) fn back(details: &mut Details) -> Vec<Effect> {
    details.drill.back();
    Vec::new()
}

/// Row `at` of the overview selected, as its menu is opened on it.
pub(crate) fn select(details: &mut Details, work: Option<&WorkSnapshot>, at: usize) -> Vec<Effect> {
    if let Some(l) = overview_listing(details, work) {
        details.drill.select(&l, at);
    }
    Vec::new()
}

/// A row of the changes or log clicked: it opens as `Enter` on it would.
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

/// A row of the overview clicked: it is selected and does what `Enter` on
/// it would, save that a heading folds or unfolds its group where it is.
pub(crate) fn click(details: &mut Details, work: Option<&WorkSnapshot>, at: usize) -> Vec<Effect> {
    let Some(l) = overview_listing(details, work) else {
        return Vec::new();
    };
    details.drill.select(&l, at);
    let Listing::Links(entries) = &l else {
        return Vec::new();
    };
    match entries.get(at) {
        Some(Entry::Heading { group, folded, .. }) => {
            fold(details, *group, !folded);
            Vec::new()
        }
        Some(Entry::More { group, .. }) => {
            details.opened.insert(*group);
            Vec::new()
        }
        Some(Entry::Agent(_) | Entry::Message(_) | Entry::Process(_)) => {
            details.drill.open(&l);
            Vec::new()
        }
        Some(entry) => used(details, entry, Use::Open),
        None => Vec::new(),
    }
}

/// `key` pressed on the panel: the cursor moves through a list shown and
/// scrolls anything else.
pub(crate) fn press(details: &mut Details, work: Option<&WorkSnapshot>, key: Key) -> Vec<Effect> {
    let shown = list(details, work);
    match (key, &shown) {
        // A row opened from the overview shows no list: the keys scroll,
        // and those that fold go back to the overview.
        (_, Some(Listing::Links(_)))
            if details.drill.agent().is_some() || details.drill.process().is_some() =>
        {
            match key {
                Key::Fold => {
                    details.drill.back();
                    Vec::new()
                }
                _ => scrolled(key),
            }
        }
        (_, Some(l @ Listing::Links(entries))) => {
            let chosen = details.drill.selected(l).map(|at| &entries[at]);
            pressed_on_overview(details, work, l, chosen, key)
        }
        (Key::Fold, _) => {
            details.drill.back();
            Vec::new()
        }
        (Key::Open, Some(l)) => {
            details.drill.open(l);
            Vec::new()
        }
        (Key::Down, Some(l)) => step(details, l, 1),
        (Key::Up, Some(l)) => step(details, l, -1),
        (Key::First, Some(l)) => {
            details.drill.first(l);
            Vec::new()
        }
        (Key::Last, Some(l)) => {
            details.drill.last(l);
            Vec::new()
        }
        (_, None) => scrolled(key),
        _ => Vec::new(),
    }
}

fn step(details: &mut Details, l: &Listing, delta: isize) -> Vec<Effect> {
    details.drill.step(l, delta);
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

fn pressed_on_overview(
    details: &mut Details,
    work: Option<&WorkSnapshot>,
    l: &Listing,
    chosen: Option<&Entry>,
    key: Key,
) -> Vec<Effect> {
    // A heading folds and unfolds as a workspace's row does; a link folds the
    // group it is in.
    let folds = match (chosen, key) {
        (Some(Entry::Heading { group, folded, .. }), Key::Open) => Some((*group, !folded)),
        (Some(Entry::Heading { group, .. }), Key::Fold) => Some((*group, true)),
        (Some(row), Key::Fold) => Some((row.group(), true)),
        _ => None,
    };
    let how = match key {
        Key::Open => Some(Use::Open),
        Key::Copy => Some(Use::Copy),
        Key::Reveal => Some(Use::Reveal),
        _ => None,
    };
    match (folds, how, chosen) {
        (Some((group, shut)), ..) => {
            fold(details, group, shut);
            if let Some(now) = overview_listing(details, work)
                && let Some(at) = now.heading(group)
            {
                details.drill.select(&now, at);
            }
            Vec::new()
        }
        (None, Some(Use::Open), Some(Entry::More { group, .. })) => {
            details.opened.insert(*group);
            Vec::new()
        }
        (None, Some(Use::Open), Some(Entry::Agent(_) | Entry::Message(_) | Entry::Process(_))) => {
            details.drill.open(l);
            Vec::new()
        }
        (None, Some(how), Some(entry)) => used(details, entry, how),
        (None, Some(_), None) => Vec::new(),
        _ => match key {
            Key::Down => step(details, l, 1),
            Key::Up => step(details, l, -1),
            Key::First => {
                details.drill.first(l);
                Vec::new()
            }
            Key::Last => {
                details.drill.last(l);
                Vec::new()
            }
            _ => Vec::new(),
        },
    }
}

/// `group` folded, when `shut`, or unfolded. A folded group forgets it was
/// shown whole, so it shows its first few when it opens again.
fn fold(details: &mut Details, group: Group, shut: bool) {
    if shut {
        details.folded.insert(group);
        details.opened.remove(&group);
    } else {
        details.folded.remove(&group);
    }
}

/// What using `entry` the way `how` says asks done.
fn used(details: &Details, entry: &Entry, how: Use) -> Vec<Effect> {
    match entry {
        Entry::Link(link) => use_link(link, how),
        Entry::Image(shot) => use_shot(shot, how, details.shown.session.as_deref()),
        Entry::Session(r) => use_related(r, how),
        Entry::Process(p) => copied(how, &p.command, "Copied the process command".into()),
        Entry::Message(n) => copied(how, &n.body, "Copied the message".into()),
        Entry::Agent(a) => copied(
            how,
            &a.prompt,
            format!("Copied the prompt of {}", a.description),
        ),
        Entry::Heading { .. } | Entry::More { .. } => Vec::new(),
    }
}

fn copied(how: Use, text: &str, done: String) -> Vec<Effect> {
    match how {
        Use::Copy => vec![Effect::Copy {
            text: text.to_owned(),
            done,
        }],
        Use::Open | Use::Reveal => Vec::new(),
    }
}

/// Open `link` in the app macOS opens it with, copy it, or show its file in
/// the Finder.
fn use_link(link: &Link, how: Use) -> Vec<Effect> {
    match how {
        Use::Open => vec![Effect::Open(link.target.clone())],
        Use::Reveal if matches!(link.kind, LinkKind::File(_)) => {
            vec![Effect::Reveal(link.target.clone())]
        }
        Use::Reveal => Vec::new(),
        Use::Copy => vec![Effect::Copy {
            text: link.target.clone(),
            done: format!("Copied {}", link.target),
        }],
    }
}

/// Show `related`, or copy its name, which Claude Code's messaging
/// addresses it by.
fn use_related(related: &Related, how: Use) -> Vec<Effect> {
    match (how, &related.id) {
        (Use::Open, Some(id)) => vec![Effect::ShowSession(id.clone())],
        (Use::Open, None) => vec![Effect::Notice(format!("{} is not running", related.name))],
        (Use::Copy, _) => vec![Effect::Copy {
            text: related.name.clone(),
            done: format!("Copied {}", related.name),
        }],
        (Use::Reveal, _) => Vec::new(),
    }
}

/// Show `shot` in a window of its own, or copy or reveal its file as a file
/// of the overview is. A drawn image is the console's of `session`.
fn use_shot(shot: &Shot, how: Use, session: Option<&str>) -> Vec<Effect> {
    match (&shot.source, how) {
        (_, Use::Open) => vec![Effect::ShowShot {
            shot: shot.clone(),
            session: session.unwrap_or_default().to_owned(),
        }],
        (Source::Console { .. }, Use::Copy) => {
            vec![Effect::Notice("A drawn image has no file to copy".into())]
        }
        (Source::Console { .. }, Use::Reveal) => Vec::new(),
        (Source::File { path, touch }, how) => use_link(
            &Link {
                kind: LinkKind::File(*touch),
                target: path.clone(),
                title: None,
                line: None,
            },
            how,
        ),
    }
}

/// What was read of the shown session.
fn session_read<'a>(details: &Details, work: Option<&'a WorkSnapshot>) -> Option<&'a SessionRead> {
    let id = details.shown.session.as_ref()?;
    work?.details.sessions.get(id)
}

/// The session shown, as the tree lists it.
pub fn shown_session<'a>(details: &Details, work: Option<&'a WorkSnapshot>) -> Option<&'a Session> {
    let id = details.shown.session.as_ref()?;
    crate::nav::find(work?.tree.tree(), id).map(|(_, s)| s)
}

/// The overview's rows for the session shown, once something was read
/// for it.
pub fn overview_listing(details: &Details, work: Option<&WorkSnapshot>) -> Option<Listing> {
    let shown = shown_session(details, work)?;
    let read = session_read(details, work);
    let links = read.and_then(|r| r.links.as_ref());
    let processes = read.and_then(|r| r.processes.as_ref());
    let notes = read.and_then(|r| r.notes.as_deref()).unwrap_or_default();
    if links.is_none() && processes.is_none() && notes.is_empty() {
        return None;
    }
    let empty = Links::default();
    let links = links.unwrap_or(&empty);
    let rows: Vec<Link> = links.rows().cloned().collect();
    let sessions: Vec<&Session> = work?.tree.tree().sessions().map(|(_, s)| s).collect();
    let ties = related(shown, &sessions, &links.peers);
    let (folded, opened) = (&details.folded, &details.opened);
    let d = &details.shown.settings;
    let entries = overview_shown(
        &ties,
        &capped(&rows, d.links_kept),
        folded,
        opened,
        d.links_shown,
    );
    let mut shots = shots(&details.shown.drawn, links, read);
    shots.truncate(d.links_kept);
    let entries = with_notes(entries, notes, folded, opened, d.links_shown);
    let entries = with_agents(entries, &links.agents, folded);
    let entries = with_processes(
        entries,
        processes.map(Vec::as_slice).unwrap_or_default(),
        folded,
        opened,
        d.links_shown,
    );
    Some(Listing::Links(with_images(
        entries,
        &shots,
        folded,
        opened,
        d.links_shown,
    )))
}

/// The overview's images: what the console `drawn`, and the image files of
/// `links` there are small copies of in `read`, newest first.
fn shots(drawn: &[Shot], links: &Links, read: Option<&SessionRead>) -> Vec<Shot> {
    let files = links.images.iter().filter_map(|f| {
        let thumb = read?.thumbs.get(&f.path)?;
        Some(Shot::file(&f.path, f.touch, f.at_ms, thumb.clone()))
    });
    newest_first(drawn.iter().cloned().chain(files).collect())
}

/// What was read of the worktree shown for the spot the panel is at.
pub fn repository(details: &Details, work: Option<&WorkSnapshot>) -> Option<View> {
    let spot = details.drill.spot()?;
    let worktree = details.shown.worktree.as_ref()?;
    let read = work?.details.repositories.get(worktree)?;
    (read.spot == spot).then(|| View::of(worktree.clone(), read.clone()))
}

/// The list the panel's cursor moves through: the overview's rows, or the
/// changes or log read; none while the panel shows no list.
pub fn list(details: &Details, work: Option<&WorkSnapshot>) -> Option<Listing> {
    if details.drill.sheet() == Sheet::Overview {
        return overview_listing(details, work);
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

/// The gauges at the panel's foot: how full the shown conversation's context
/// is, then the account's limits at `now_ms`.
pub fn usage(details: &Details, work: Option<&WorkSnapshot>, now_ms: u64) -> Vec<Gauge> {
    let mut out = match (details.shown.usage, work) {
        (Some(agent), Some(work)) => work
            .details
            .usage
            .get(&agent)
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

/// The subagent of the overview opened.
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

/// The message of the overview opened.
pub fn opened_note(details: &Details, work: Option<&WorkSnapshot>) -> Option<Note> {
    let id = details.drill.message()?;
    session_read(details, work)?
        .notes
        .as_ref()?
        .iter()
        .find(|n| n.id == id)
        .cloned()
}

/// The process of the overview opened.
pub fn opened_process(details: &Details, work: Option<&WorkSnapshot>) -> Option<Process> {
    let key = details.drill.process()?;
    session_read(details, work)?
        .processes
        .as_ref()?
        .iter()
        .find(|p| p.key == key)
        .cloned()
}

/// What the host keeps read for the panel: the shown session's
/// conversation and processes, the spot of the worktree shown, and the
/// account's usage.
pub(crate) fn sight(details: &Details) -> Sight {
    let shown = &details.shown;
    Sight {
        session: shown.session.clone(),
        details: shown.open,
        overview: details.drill.sheet() == Sheet::Overview,
        repository: details
            .drill
            .spot()
            .zip(shown.worktree.clone())
            .map(|(spot, worktree)| RepositorySight { worktree, spot }),
        usage: shown.usage,
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

/// A subagent or process opened that the overview no longer lists is gone
/// back from.
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
    });
    if gone {
        details.drill.back();
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;
    use std::sync::Arc;

    use adroit_model::links::{Peer, Touch};
    use adroit_model::repository::{DiffKind, DiffLine, Listing as Read, Rev};
    use adroit_model::subagents::{Agent as Subagent, Standing};
    use adroit_model::usage::{Limit, Severity, Window};
    use adroit_protocol::{
        Change, ContextRead, ConversationRead, DetailsChange, Event, Listed, RepositoryRead,
        Request, TreeWork, WorkPatch,
    };

    use super::*;
    use crate::msg::Msg;
    use crate::nav::Place;
    use crate::scenario::Scenario;

    fn session(id: &str) -> Session {
        Session {
            id: id.into(),
            name: id.into(),
            session_id: format!("{id}-conversation"),
            ..Default::default()
        }
    }

    /// The tree the host lists, holding every session these tests show.
    fn listed() -> TreeWork {
        let sessions: Vec<Session> = ["a", "b", "s1", "s2", "session-one", "session-two"]
            .into_iter()
            .map(|id| Session {
                cwd: "/w".into(),
                ..session(id)
            })
            .collect();
        let tree = adroit_model::build(&sessions, &[], &Default::default(), &[], None);
        TreeWork {
            listed: Some(Box::new(Listed {
                sessions,
                terminals: Vec::new(),
                tree,
            })),
            ..Default::default()
        }
    }

    fn shown(id: Option<&str>, worktree: Option<&str>) -> Msg {
        Msg::Showing(Box::new(Showing {
            tab: id.map(|id| Tab::Session(id.into())),
            place: worktree.map(|w| Place { worktree: w.into() }),
        }))
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

    fn snapshot(details: DetailsWork) -> Event {
        Event::Snapshot(Box::new(WorkSnapshot {
            seq: Seq(1),
            details,
            tree: listed(),
        }))
    }

    fn patch(seq: u64, changes: Vec<DetailsChange>) -> Event {
        Event::Patch(WorkPatch {
            seq: Seq(seq),
            changes: changes.into_iter().map(Change::Details).collect(),
        })
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
                Effect::Host(Request::Watch(sight)) => Some((**sight).clone()),
                _ => None,
            })
            .collect()
    }

    fn local(run: &Scenario) -> Vec<Effect> {
        run.last_effects()
            .iter()
            .filter(|e| !matches!(e, Effect::Host(_)))
            .cloned()
            .collect()
    }

    fn overview_of(run: &Scenario) -> Option<Listing> {
        overview_listing(&run.state().details, run.state().host.work.as_ref())
    }

    #[test]
    fn the_host_is_asked_to_read_what_the_panel_shows_once_each_time_it_changes() {
        let run = Scenario::new().send_all([
            shown(Some("a"), Some("/w")),
            shown(Some("a"), Some("/w")),
            Msg::DetailsSheet(Sheet::Log),
            Msg::DetailsSheet(Sheet::Log),
            shown(None, Some("/w")),
        ]);
        let sights: Vec<_> = watched(&run)
            .into_iter()
            .map(|s| {
                (
                    s.session,
                    s.overview,
                    s.repository.map(|r| (r.worktree, r.spot)),
                )
            })
            .collect();
        assert_eq!(
            sights,
            [
                (Some("a".into()), true, None),
                (Some("a".into()), false, Some(("/w".into(), Spot::Log))),
                (None, false, Some(("/w".into(), Spot::Log))),
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
        let opened = Scenario::new()
            .event(snapshot(DetailsWork::default()))
            .send_all([shown(None, Some("/w")), Msg::DetailsSheet(Sheet::Log)])
            .event(patch(
                2,
                vec![DetailsChange::Repository {
                    worktree: "/w".into(),
                    read: log,
                }],
            ))
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
        assert_eq!(moved.state().details.drill, Drill::new(Sheet::Log));
    }

    #[test]
    fn the_sheet_keys_choose_a_sheet_and_step_along_from_its_top() {
        let cases: Vec<(Vec<Msg>, Sheet)> = vec![
            (vec![], Sheet::Overview),
            (vec![Msg::DetailsSheet(Sheet::Changes)], Sheet::Changes),
            (vec![Msg::DetailsSheetStep(1)], Sheet::Changes),
            (vec![Msg::DetailsSheetStep(-1)], Sheet::Log),
            (
                vec![Msg::DetailsSheet(Sheet::Log), Msg::DetailsSheetStep(1)],
                Sheet::Overview,
            ),
        ];
        for (msgs, sheet) in cases {
            let run = Scenario::new().send_all(msgs.clone());
            assert_eq!(run.state().details.drill, Drill::new(sheet), "{msgs:?}");
        }
    }

    #[test]
    fn the_keys_on_the_overview_use_the_row_selected() {
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
                vec![Effect::Open("https://a.dev".into())],
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
                vec![Effect::Reveal("/w/a.rs".into())],
            ),
            (
                vec![Key::Last, Key::Open],
                vec![Effect::Open("/w/a.rs".into())],
            ),
            (vec![Key::Copy], vec![]),
        ];
        for (keys, want) in cases {
            let run = Scenario::new()
                .event(snapshot(read("a", with_links(links.clone()))))
                .send(shown(Some("a"), None))
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
        let running = Scenario::new()
            .event(snapshot(read("a", with_links(peers("b")))))
            .send(shown(Some("a"), None))
            .send_all([Msg::DetailsKey(Key::Down), Msg::DetailsKey(Key::Open)]);
        assert_eq!(local(&running), [Effect::ShowSession("b".into())]);
        let gone = Scenario::new()
            .event(snapshot(read("a", with_links(peers("gone")))))
            .send(shown(Some("a"), None))
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
        let run = Scenario::new()
            .event(snapshot(read("a", with_links(links))))
            .send(shown(Some("a"), None))
            .send_all([Msg::DetailsKey(Key::Down), Msg::DetailsKey(Key::Fold)]);
        let d = &run.state().details;
        assert_eq!(d.folded, HashSet::from([Group::Links]));
        let listing = overview_of(&run).unwrap();
        assert_eq!(d.drill.selected(&listing), listing.heading(Group::Links));
        let unfolded = run.send(Msg::DetailsKey(Key::Open));
        assert!(unfolded.state().details.folded.is_empty());
    }

    #[test]
    fn a_group_folded_shows_its_first_few_again_once_unfolded() {
        let links = Links {
            files: (0..8)
                .map(|i| link(LinkKind::File(Touch::Read), &format!("/w/{i}.rs")))
                .collect(),
            ..Default::default()
        };
        let mut told_now = Told {
            open: true,
            ..Default::default()
        };
        told_now.settings.links_shown = 2;
        let run = Scenario::new()
            .event(snapshot(read("a", with_links(links))))
            .send_all([shown(Some("a"), None), told(told_now)]);
        // Rows: heading, two files, the row to show all.
        let all = run.send(Msg::DetailsClick(3));
        assert_eq!(all.state().details.opened, HashSet::from([Group::Files]));
        let folded = all.send(Msg::DetailsClick(0));
        assert!(folded.state().details.opened.is_empty());
        let unfolded = folded.send(Msg::DetailsClick(0));
        let Some(Listing::Links(entries)) = overview_of(&unfolded) else {
            panic!("no overview");
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
        let run = Scenario::new()
            .event(snapshot(read("session-one", processes)))
            .send(shown(Some("session-one"), None));
        let Some(Listing::Links(entries)) = overview_of(&run) else {
            panic!("processes without transcript links")
        };
        assert!(
            entries
                .iter()
                .any(|e| matches!(e, Entry::Process(p) if p.pid == 123))
        );
        assert!(overview_of(&run.send(shown(Some("session-two"), None))).is_none());
    }

    #[test]
    fn the_overview_lists_drawn_images_of_the_session_shown_after_its_artifacts() {
        let links = Links {
            artifacts: vec![link(LinkKind::Artifact, "https://claude.ai/artifact/a")],
            urls: vec![link(LinkKind::Url, "https://a.dev")],
            ..Default::default()
        };
        let run = Scenario::new()
            .event(snapshot(read("s1", with_links(links))))
            .send_all([
                shown(Some("s1"), None),
                told(Told {
                    drawn: vec![Shot::console(1, 1, 1, 5, None)],
                    ..Default::default()
                }),
            ]);
        let Some(Listing::Links(entries)) = overview_of(&run) else {
            panic!("no overview");
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
    fn the_overview_shows_the_image_files_it_has_small_copies_of_newest_first() {
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
    fn the_overview_lists_links_only_for_the_session_they_were_read_for() {
        let links = Links {
            urls: vec![link(LinkKind::Url, "https://a.dev")],
            ..Default::default()
        };
        let run = Scenario::new().event(snapshot(read("s1", with_links(links))));
        assert!(overview_of(&run.send(shown(Some("s1"), None))).is_some());
        let run = Scenario::new().event(snapshot(read(
            "s1",
            with_links(Links {
                urls: vec![link(LinkKind::Url, "https://a.dev")],
                ..Default::default()
            }),
        )));
        assert!(overview_of(&run.send(shown(Some("s2"), None))).is_none());
        let run = Scenario::new().event(snapshot(read("s1", SessionRead::default())));
        assert!(overview_of(&run.send(shown(None, None))).is_none());
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
        let opened = Scenario::new()
            .event(snapshot(DetailsWork::default()))
            .send_all([shown(None, Some("/w")), Msg::DetailsSheet(Sheet::Changes)])
            .event(patch(
                2,
                vec![DetailsChange::Repository {
                    worktree: "/w".into(),
                    read: files,
                }],
            ))
            .send(Msg::DetailsOpenRow(0));
        let diffing = Some(Spot::Diff(Rev::WorkingCopy, file.clone()));
        assert_eq!(opened.state().details.drill.spot(), diffing);
        let changed = opened.event(patch(
            3,
            vec![diff(vec![DiffLine {
                kind: DiffKind::Added,
                text: "+x".into(),
            }])],
        ));
        assert_eq!(changed.state().details.drill.spot(), diffing);
        let unchanged = changed.event(patch(4, vec![diff(vec![])]));
        assert_eq!(
            unchanged.state().details.drill.spot(),
            Some(Spot::Files(Rev::WorkingCopy))
        );
    }

    #[test]
    fn a_subagent_gone_from_the_overview_is_gone_back_from() {
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
        let opened = Scenario::new()
            .event(snapshot(read("a", with_links(links))))
            .send(shown(Some("a"), None))
            .send_all([Msg::DetailsKey(Key::Down), Msg::DetailsKey(Key::Open)]);
        assert_eq!(opened.state().details.drill.agent(), Some("toolu_1"));
        assert!(opened_agent(&opened.state().details, opened.state().host.work.as_ref()).is_some());
        let scrolled = opened.send(Msg::DetailsKey(Key::Down));
        assert_eq!(local(&scrolled), [Effect::ScrollDetails(Scroll::Down)]);
        let gone = scrolled.event(patch(
            2,
            vec![DetailsChange::Links {
                session: "a".into(),
                links: Links::default(),
            }],
        ));
        assert_eq!(gone.state().details.drill.agent(), None);
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
            let run = Scenario::new().send(Msg::DetailsKey(key));
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
            let mut work = read(
                "a",
                SessionRead {
                    conversation: Some(conversation(read_id)),
                    context: Some(asked(asked_id)),
                    ..Default::default()
                },
            );
            work.usage.insert(Agent::Claude, limits.clone());
            let run = Scenario::new().event(snapshot(work)).send_all([
                shown(Some("a"), None),
                told(Told {
                    usage: usage_of,
                    ..Default::default()
                }),
            ]);
            let got = usage(&run.state().details, run.state().host.work.as_ref(), 0);
            assert_eq!(
                (got.len(), got[0].context),
                (count, context),
                "{read_id} {asked_id} {usage_of:?}"
            );
        }
    }
}
