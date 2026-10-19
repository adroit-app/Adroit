//! The sheets of the details panel.

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use adroit_claude::git::{Change, Commit, DiffLine};
use adroit_claude::links::{Kind as LinkKind, Link, Peer, Touch};
use adroit_claude::subagents::{Agent, Standing};
use adroit_claude::usage::{Limit, Severity, Window};
use adroit_model::{Kind, Session, State};

/// What the details panel says about a session, as label and value, at
/// `now_ms` since the Unix epoch.
pub fn facts(s: &Session, now_ms: u64) -> Vec<(&'static str, String)> {
    let state = match &s.state {
        State::Waiting(why) => format!("waiting · {why}"),
        other => other.word().to_string(),
    };
    let runs = match (s.kind, s.reads_only()) {
        (Kind::Own, false) => "in adroit",
        (Kind::Own, true) => "in adroit, reading only",
        (Kind::Interactive, false) => "in a terminal",
        (Kind::Interactive, true) => "in a terminal, reading only",
    };
    let mut out = vec![("State", state), ("Runs", runs.to_string())];
    if let Some(started) = s.started_ms {
        out.push(("Started", ago(now_ms.saturating_sub(started))));
    }
    out
}

/// `ms` milliseconds back, in the largest whole unit.
fn ago(ms: u64) -> String {
    let min = ms / 60_000;
    match min {
        0 => "just now".into(),
        1..60 => format!("{min} min ago"),
        60..1440 => format!("{} h ago", min / 60),
        _ => match min / 1440 {
            1 => "1 day ago".into(),
            d => format!("{d} days ago"),
        },
    }
}

/// A usage limit as the foot of the overview draws it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Gauge {
    pub label: String,
    /// How much of the bar is filled, from 0 to 100.
    pub used: u32,
    pub left: String,
    pub note: String,
    pub severity: Severity,
}

/// The gauges for `limits`, at `now_ms` since the Unix epoch.
pub fn gauges(limits: &[Limit], now_ms: u64) -> Vec<Gauge> {
    limits
        .iter()
        .map(|l| {
            let label = match &l.window {
                Window::Session => "Session".to_string(),
                Window::Week => "Week".to_string(),
                Window::WeekOf(name) => format!("Week · {name}"),
                Window::Other(kind) => kind.replace('_', " "),
            };
            let (percent, note) = match l.resets_ms {
                Some(at) if at <= now_ms => (0, "reset".to_string()),
                resets => {
                    let percent = l.percent.max(0.0).round() as u32;
                    let mut note = format!("{percent}% used");
                    if let Some(at) = resets {
                        note.push_str(&format!(" · resets {}", until(at - now_ms)));
                    }
                    (percent, note)
                }
            };
            Gauge {
                label,
                used: percent.min(100),
                left: format!("{}% left", 100u32.saturating_sub(percent)),
                note,
                severity: l.severity,
            }
        })
        .collect()
}

/// `ms` milliseconds ahead, in the two largest whole units.
fn until(ms: u64) -> String {
    let min = ms / 60_000;
    let (h, m) = (min / 60, min % 60);
    let (d, dh) = (h / 24, h % 24);
    let days = match d {
        1 => "1 day".to_string(),
        d => format!("{d} days"),
    };
    match (d, h, m) {
        (0, 0, 0) => "in under a minute".into(),
        (0, 0, m) => format!("in {m} min"),
        (0, h, 0) => format!("in {h} h"),
        (0, h, m) => format!("in {h} h {m} min"),
        (_, _, _) if dh == 0 => format!("in {days}"),
        _ => format!("in {days} {dh} h"),
    }
}

/// What a link of the overview shows: what it is called, and a word on it.
pub fn shown_link(link: &Link, worktree: &Path, home: &Path) -> (String, String) {
    let id = || adroit_claude::links::artifact_id(&link.target).unwrap_or(&link.target);
    match link.kind {
        LinkKind::Artifact => match &link.title {
            Some(t) => (t.clone(), id().to_string()),
            None => (id().to_string(), String::new()),
        },
        LinkKind::Url => {
            let bare = link
                .target
                .split_once("://")
                .map_or(&*link.target, |(_, r)| r);
            let host = bare.split(['/', '?', '#']).next().unwrap_or_default();
            let label = link.title.clone().unwrap_or_else(|| bare.to_string());
            (label, host.to_string())
        }
        LinkKind::File(touch) => {
            let path = Path::new(&link.target);
            let mut label = match (path.strip_prefix(worktree), path.strip_prefix(home)) {
                (Ok(rel), _) => rel.display().to_string(),
                (_, Ok(rel)) => format!("~/{}", rel.display()),
                _ => link.target.clone(),
            };
            if let Some(line) = link.line {
                label.push_str(&format!(":{line}"));
            }
            let word = match touch {
                Touch::Edited => "edited",
                Touch::Read => "read",
                Touch::Mentioned => "mentioned",
            };
            (label, word.to_string())
        }
    }
}

/// Where an image of the overview came from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Source {
    /// The console drew it: kept by its id, `width` by `height` pixels.
    Console { id: u64, width: u32, height: u32 },
    /// The session came across the file at `path`.
    File { path: String, touch: Touch },
}

/// An image of the overview.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Shot {
    /// What tells it apart from every other row.
    pub key: String,
    pub source: Source,
    /// When it came up, in milliseconds since the Unix epoch.
    pub at_ms: Option<u64>,
    /// A small copy as a `data:` URL, when one could be made.
    pub thumb: Option<Arc<str>>,
}

impl Shot {
    pub fn console(id: u64, width: u32, height: u32, at_ms: u64, thumb: Option<Arc<str>>) -> Shot {
        Shot {
            // No path or address starts with a NUL, so no other row takes a
            // drawn image's place.
            key: format!("\0drawn {id:016x}"),
            source: Source::Console { id, width, height },
            at_ms: Some(at_ms),
            thumb,
        }
    }

    pub fn file(path: &str, touch: Touch, at_ms: Option<u64>, thumb: Option<Arc<str>>) -> Shot {
        Shot {
            key: path.to_string(),
            source: Source::File {
                path: path.to_string(),
                touch,
            },
            at_ms,
            thumb,
        }
    }
}

/// `shots` newest first; those of an unknown time keep their order last.
pub fn newest_first(mut shots: Vec<Shot>) -> Vec<Shot> {
    shots.sort_by_key(|s| (s.at_ms.is_none(), std::cmp::Reverse(s.at_ms)));
    shots
}

/// What an image of the overview shows: what it is called, and a word on
/// how the session came by it.
pub fn shown_shot(shot: &Shot, worktree: &Path, home: &Path) -> (String, String) {
    match &shot.source {
        Source::Console { width, height, .. } => {
            (format!("Image {width} × {height}"), "drawn".to_string())
        }
        Source::File { path, touch } => shown_link(
            &Link {
                kind: LinkKind::File(*touch),
                target: path.clone(),
                title: None,
                line: None,
            },
            worktree,
            home,
        ),
    }
}

/// What a subagent of the overview shows: what it is called now, and how it
/// stands, how long it ran told at `now_ms`.
pub fn shown_agent(a: &Agent, now_ms: u64) -> (String, String) {
    let label = match (&a.title, a.description.as_str()) {
        (Some(title), _) => title.clone(),
        (None, "") => "subagent".to_string(),
        (None, description) => description.to_string(),
    };
    let took = a
        .started_ms
        .map(|start| lasted(a.ended_ms.unwrap_or(now_ms).saturating_sub(start)));
    let tag = match (a.standing, took) {
        (Standing::Running, Some(t)) => format!("running {t}"),
        (Standing::Done, Some(t)) => format!("done in {t}"),
        (Standing::Failed, Some(t)) => format!("failed after {t}"),
        (Standing::Stopped, Some(t)) => format!("stopped after {t}"),
        (s, None) => standing_word(s).to_string(),
    };
    (label, tag)
}

pub fn standing_word(s: Standing) -> &'static str {
    match s {
        Standing::Running => "running",
        Standing::Done => "done",
        Standing::Failed => "failed",
        Standing::Stopped => "stopped",
    }
}

/// `ms` milliseconds as seconds, minutes, or hours and minutes.
fn lasted(ms: u64) -> String {
    let s = ms / 1000;
    match (s / 3600, s / 60 % 60) {
        (0, 0) => format!("{s} s"),
        (0, m) => format!("{m} min"),
        (h, 0) => format!("{h} h"),
        (h, m) => format!("{h} h {m} min"),
    }
}

/// What the details panel shows.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Sheet {
    /// The session shown, where it works, and the artifacts, links and files
    /// its conversation brought up, each opening outside adroit.
    Overview,
    /// The files the working copy changes, each opening its diff.
    Changes,
    /// The recent commits, or jj's stack, each opening the files it changed.
    Log,
}

impl Sheet {
    pub const ALL: [Sheet; 3] = [Sheet::Overview, Sheet::Changes, Sheet::Log];

    pub fn title(self) -> &'static str {
        match self {
            Sheet::Overview => "Overview",
            Sheet::Changes => "Changes",
            Sheet::Log => "Log",
        }
    }

    /// The sheet `delta` places along, wrapping around.
    pub fn step(self, delta: isize) -> Sheet {
        let at = Self::ALL.iter().position(|s| *s == self).unwrap_or(0);
        Self::ALL[crate::nav::wrap(at, delta, Self::ALL.len())]
    }
}

/// Where changes are read from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Rev {
    /// What is being worked on: git's worktree, or jj's `@`.
    WorkingCopy,
    /// A commit, or a jj change, by its [`Commit::id`].
    Commit(String),
}

/// What a level of a sheet shows.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Spot {
    Files(Rev),
    Diff(Rev, Change),
    Log,
}

/// What was read for a spot.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Listing {
    Files(Vec<Change>),
    Log(Vec<Commit>),
    Diff(Vec<DiffLine>),
    Error(String),
    /// The overview's rows, in the order it shows them.
    Links(Vec<Entry>),
}

impl Listing {
    /// Whether it lists rows to select and open.
    pub fn is_list(&self) -> bool {
        matches!(
            self,
            Listing::Files(_) | Listing::Log(_) | Listing::Links(_)
        )
    }

    /// The row of the overview that heads `group`.
    pub fn heading(&self, group: Group) -> Option<usize> {
        let Listing::Links(entries) = self else {
            return None;
        };
        entries
            .iter()
            .position(|e| matches!(e, Entry::Heading { group: g, .. } if *g == group))
    }

    /// What tells each row apart, for a list; nothing for the rest.
    fn keys(&self) -> Vec<&str> {
        match self {
            Listing::Files(files) => files.iter().map(|f| f.path.as_str()).collect(),
            Listing::Log(commits) => commits.iter().map(|c| c.id.as_str()).collect(),
            Listing::Links(entries) => entries.iter().map(Entry::key).collect(),
            Listing::Diff(_) | Listing::Error(_) => Vec::new(),
        }
    }
}

/// A group of the overview's links, shown under a heading of its own.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Group {
    Sessions,
    Agents,
    Artifacts,
    Images,
    Links,
    Files,
}

impl Group {
    pub fn of(kind: &LinkKind) -> Group {
        match kind {
            LinkKind::Artifact => Group::Artifacts,
            LinkKind::Url => Group::Links,
            LinkKind::File(_) => Group::Files,
        }
    }

    pub fn title(self) -> &'static str {
        match self {
            Group::Sessions => "Sessions",
            Group::Agents => "Subagents",
            Group::Artifacts => "Artifacts",
            Group::Images => "Images",
            Group::Links => "Links",
            Group::Files => "Files",
        }
    }
}

/// A row of the overview: the heading of a group, or one of its links.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Entry {
    Heading {
        group: Group,
        count: usize,
        folded: bool,
    },
    Session(Related),
    Agent(Agent),
    Link(Link),
    Image(Shot),
    /// The row standing for a group's links not shown, which shows them all.
    More {
        group: Group,
        hidden: usize,
    },
}

impl Entry {
    fn key(&self) -> &str {
        match self {
            // No address, path or session name starts with a NUL, so no row
            // takes a heading's place.
            Entry::Heading { group, .. } => match group {
                Group::Sessions => "\0sessions",
                Group::Agents => "\0agents",
                Group::Artifacts => "\0artifacts",
                Group::Images => "\0images",
                Group::Links => "\0links",
                Group::Files => "\0files",
            },
            Entry::Session(r) => r.name.as_str(),
            Entry::Agent(a) => a.call.as_str(),
            Entry::Link(l) => l.target.as_str(),
            Entry::Image(s) => s.key.as_str(),
            Entry::More { group, .. } => match group {
                Group::Sessions => "\0more sessions",
                Group::Agents => "\0more agents",
                Group::Artifacts => "\0more artifacts",
                Group::Images => "\0more images",
                Group::Links => "\0more links",
                Group::Files => "\0more files",
            },
        }
    }

    /// The group the row is in.
    pub fn group(&self) -> Group {
        match self {
            Entry::Heading { group, .. } => *group,
            Entry::Session(_) => Group::Sessions,
            Entry::Agent(_) => Group::Agents,
            Entry::Link(l) => Group::of(&l.kind),
            Entry::Image(_) => Group::Images,
            Entry::More { group, .. } => *group,
        }
    }
}

/// How another session is tied to the one shown.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tie {
    /// It started the one shown.
    StartedBy,
    /// The one shown started it.
    Started,
    /// The one shown messaged it.
    Sent,
    /// It messaged the one shown.
    Received,
}

impl Tie {
    pub fn word(self) -> &'static str {
        match self {
            Tie::StartedBy => "started this",
            Tie::Started => "started",
            Tie::Sent => "messaged",
            Tie::Received => "heard from",
        }
    }
}

/// Another session the one shown worked with.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Related {
    pub name: String,
    /// Which session it is, while one runs under its name.
    pub id: Option<String>,
    pub ties: Vec<Tie>,
}

/// The sessions `shown` worked with, among `sessions`: the one it was started
/// from, those it started, then those it messaged or heard from, newest
/// first, as `peers` gives them.
pub fn related(shown: &Session, sessions: &[&Session], peers: &[Peer]) -> Vec<Related> {
    let mut out: Vec<Related> = Vec::new();
    let mut tie = |name: &str, id: Option<&str>, tie: Tie| {
        let at = out.iter().position(|r| match (&r.id, id) {
            (Some(a), Some(b)) => a == b,
            _ => r.name == name,
        });
        match at {
            Some(at) => out[at].ties.push(tie),
            None => out.push(Related {
                name: name.to_string(),
                id: id.map(str::to_string),
                ties: vec![tie],
            }),
        }
    };
    let others = || sessions.iter().filter(|s| s.id != shown.id);
    if !shown.started_from.is_empty()
        && let Some(s) = others().find(|s| s.session_id == shown.started_from)
    {
        tie(&s.name, Some(&s.id), Tie::StartedBy);
    }
    if !shown.session_id.is_empty() {
        for s in others().filter(|s| s.started_from == shown.session_id) {
            tie(&s.name, Some(&s.id), Tie::Started);
        }
    }
    for p in peers.iter().filter(|p| p.name != shown.name) {
        let id = others().find(|s| s.name == p.name).map(|s| s.id.as_str());
        if p.sent {
            tie(&p.name, id, Tie::Sent);
        }
        if p.received {
            tie(&p.name, id, Tie::Received);
        }
    }
    out
}

/// `links`, which come grouped and newest first, keeping up to `kept` of each
/// group.
pub fn capped(links: &[Link], kept: usize) -> Vec<Link> {
    links
        .chunk_by(|a, b| Group::of(&a.kind) == Group::of(&b.kind))
        .flat_map(|run| run.iter().take(kept).cloned())
        .collect()
}

/// The overview's rows: the `related` sessions under their heading, then
/// `links`, which come grouped, each group's heading followed by its links;
/// a `folded` group shows its heading alone.
#[cfg(test)]
pub fn overview(related: &[Related], links: &[Link], folded: &HashSet<Group>) -> Vec<Entry> {
    overview_shown(related, links, folded, &HashSet::new(), usize::MAX)
}

/// [`overview`], each group of links showing its `first` links and a row for
/// the rest, unless it is among those `opened` to show them all.
pub fn overview_shown(
    related: &[Related],
    links: &[Link],
    folded: &HashSet<Group>,
    opened: &HashSet<Group>,
    first: usize,
) -> Vec<Entry> {
    let mut out = Vec::new();
    if !related.is_empty() {
        let shut = folded.contains(&Group::Sessions);
        out.push(Entry::Heading {
            group: Group::Sessions,
            count: related.len(),
            folded: shut,
        });
        if !shut {
            out.extend(related.iter().cloned().map(Entry::Session));
        }
    }
    for run in links.chunk_by(|a, b| Group::of(&a.kind) == Group::of(&b.kind)) {
        let group = Group::of(&run[0].kind);
        let shut = folded.contains(&group);
        out.push(Entry::Heading {
            group,
            count: run.len(),
            folded: shut,
        });
        if !shut {
            let shown = if opened.contains(&group) {
                run.len()
            } else {
                first.min(run.len())
            };
            out.extend(run[..shown].iter().cloned().map(Entry::Link));
            if shown < run.len() {
                out.push(Entry::More {
                    group,
                    hidden: run.len() - shown,
                });
            }
        }
    }
    out
}

/// The overview's `entries` with `agents` under their heading after the
/// sessions; a `folded` group shows its heading alone.
pub fn with_agents(
    mut entries: Vec<Entry>,
    agents: &[Agent],
    folded: &HashSet<Group>,
) -> Vec<Entry> {
    if agents.is_empty() {
        return entries;
    }
    let at = entries
        .iter()
        .position(|e| e.group() != Group::Sessions)
        .unwrap_or(entries.len());
    let shut = folded.contains(&Group::Agents);
    let mut rows = vec![Entry::Heading {
        group: Group::Agents,
        count: agents.len(),
        folded: shut,
    }];
    if !shut {
        rows.extend(agents.iter().cloned().map(Entry::Agent));
    }
    entries.splice(at..at, rows);
    entries
}

/// The overview's `entries` with `shots` under their heading after the
/// artifacts, showing their `first` and a row for the rest unless the group
/// is among those `opened`; a `folded` group shows its heading alone.
pub fn with_images(
    mut entries: Vec<Entry>,
    shots: &[Shot],
    folded: &HashSet<Group>,
    opened: &HashSet<Group>,
    first: usize,
) -> Vec<Entry> {
    if shots.is_empty() {
        return entries;
    }
    let at = entries
        .iter()
        .position(|e| matches!(e.group(), Group::Links | Group::Files))
        .unwrap_or(entries.len());
    let shut = folded.contains(&Group::Images);
    let mut rows = vec![Entry::Heading {
        group: Group::Images,
        count: shots.len(),
        folded: shut,
    }];
    if !shut {
        let shown = if opened.contains(&Group::Images) {
            shots.len()
        } else {
            first.min(shots.len())
        };
        rows.extend(shots[..shown].iter().cloned().map(Entry::Image));
        if shown < shots.len() {
            rows.push(Entry::More {
                group: Group::Images,
                hidden: shots.len() - shown,
            });
        }
    }
    entries.splice(at..at, rows);
    entries
}

/// What was read for a spot, and which worktree it was about.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct View {
    pub spot: Spot,
    pub worktree: PathBuf,
    pub listing: Listing,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum Opened {
    Commit(Commit),
    File(Change),
    /// A subagent of the overview, by its call, and its description.
    Agent(String, String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct Cursor {
    key: String,
    at: usize,
}

/// How far into a sheet the panel has gone: the rows opened, and the row
/// selected in what is shown.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Drill {
    sheet: Sheet,
    opened: Vec<Opened>,
    cursor: Option<Cursor>,
}

impl Drill {
    pub fn new(sheet: Sheet) -> Self {
        Drill {
            sheet,
            opened: Vec::new(),
            cursor: None,
        }
    }

    pub fn sheet(&self) -> Sheet {
        self.sheet
    }

    /// The subagent of the overview opened, by its call.
    pub fn agent(&self) -> Option<&str> {
        match self.opened.last() {
            Some(Opened::Agent(call, _)) => Some(call),
            _ => None,
        }
    }

    /// What to read for what is shown; the overview is not read.
    pub fn spot(&self) -> Option<Spot> {
        let rev = |c: &Commit| Rev::Commit(c.id.clone());
        Some(match (self.sheet, self.opened.as_slice()) {
            (Sheet::Overview, _) => return None,
            (Sheet::Changes, []) => Spot::Files(Rev::WorkingCopy),
            (Sheet::Changes, [Opened::File(f), ..]) => Spot::Diff(Rev::WorkingCopy, f.clone()),
            (Sheet::Log, []) => Spot::Log,
            (Sheet::Log, [Opened::Commit(c)]) => Spot::Files(rev(c)),
            (Sheet::Log, [Opened::Commit(c), Opened::File(f), ..]) => Spot::Diff(rev(c), f.clone()),
            _ => return None,
        })
    }

    /// What each opened row is called, outermost first.
    pub fn crumbs(&self) -> Vec<String> {
        self.opened
            .iter()
            .map(|o| match o {
                Opened::Commit(c) if c.subject.is_empty() => {
                    format!("{} (no description)", c.short)
                }
                Opened::Commit(c) => format!("{} {}", c.short, c.subject),
                Opened::File(f) => f.shown(),
                Opened::Agent(_, description) => description.clone(),
            })
            .collect()
    }

    /// The row of `listing` selected.
    pub fn selected(&self, listing: &Listing) -> Option<usize> {
        let keys = listing.keys();
        if keys.is_empty() {
            return None;
        }
        Some(match &self.cursor {
            None => 0,
            Some(c) => keys
                .iter()
                .position(|k| *k == c.key)
                .unwrap_or(c.at.min(keys.len() - 1)),
        })
    }

    /// Select row `at` of `listing`.
    pub fn select(&mut self, listing: &Listing, at: usize) {
        self.cursor = listing.keys().get(at).map(|k| Cursor {
            key: k.to_string(),
            at,
        });
    }

    /// Move the selection `delta` rows, stopping at either end.
    pub fn step(&mut self, listing: &Listing, delta: isize) {
        let Some(at) = self.selected(listing) else {
            return;
        };
        let last = listing.keys().len() - 1;
        let to = at.saturating_add_signed(delta).min(last);
        self.select(listing, to);
    }

    pub fn first(&mut self, listing: &Listing) {
        self.select(listing, 0);
    }

    pub fn last(&mut self, listing: &Listing) {
        self.select(listing, listing.keys().len().saturating_sub(1));
    }

    /// Open the selected row; whether there was one to open.
    pub fn open(&mut self, listing: &Listing) -> bool {
        let Some(at) = self.selected(listing) else {
            return false;
        };
        let opened = match listing {
            Listing::Files(files) => Opened::File(files[at].clone()),
            Listing::Log(commits) => Opened::Commit(commits[at].clone()),
            Listing::Links(entries) => match &entries[at] {
                Entry::Agent(a) => Opened::Agent(a.call.clone(), shown_agent(a, 0).0),
                _ => return false,
            },
            Listing::Diff(_) | Listing::Error(_) => return false,
        };
        self.opened.push(opened);
        self.cursor = None;
        true
    }

    /// Open row `at`; whether there was one.
    pub fn open_at(&mut self, listing: &Listing, at: usize) -> bool {
        if at >= listing.keys().len() {
            return false;
        }
        self.select(listing, at);
        self.open(listing)
    }

    /// Go back out to the list the shown row was opened from, with it
    /// selected; whether there was one.
    pub fn back(&mut self) -> bool {
        let Some(opened) = self.opened.pop() else {
            return false;
        };
        let key = match opened {
            Opened::Commit(c) => c.id,
            Opened::File(f) => f.path,
            Opened::Agent(call, _) => call,
        };
        // Where it was is not kept; should it be gone meanwhile, the list's
        // first row stands in.
        self.cursor = Some(Cursor { key, at: 0 });
        true
    }

    /// Take in what was read again for the spot shown: the selection follows
    /// its row, or stays where it was when the row is gone. A row opened that
    /// is gone, its revision unknown now or its file changed no more, is
    /// stepped back from; whether it was.
    pub fn refreshed(&mut self, listing: &Listing) -> bool {
        let gone = match listing {
            Listing::Error(_) => true,
            Listing::Diff(lines) => lines.is_empty(),
            Listing::Files(_) | Listing::Log(_) | Listing::Links(_) => false,
        };
        if gone && !self.opened.is_empty() {
            return self.back();
        }
        if self.cursor.is_some()
            && let Some(at) = self.selected(listing)
        {
            self.select(listing, at);
        }
        false
    }
}

/// Read what `spot` shows of `worktree`: through jj in a jj repository,
/// through git otherwise or when jj cannot be run.
pub async fn load(spot: Spot, worktree: PathBuf) -> View {
    use adroit_claude::{Error, git, jj};
    let diff_lines = crate::settings::current().details.diff_lines;
    let mut listing = None;
    if jj::is_jj(&worktree) {
        let rev = |r: &Rev| match r {
            Rev::WorkingCopy => "@".to_string(),
            Rev::Commit(id) => id.clone(),
        };
        let read = match &spot {
            Spot::Files(r) => jj::files(&worktree, &rev(r)).await.map(Listing::Files),
            Spot::Diff(r, c) => jj::file_diff(&worktree, &rev(r), c, diff_lines)
                .await
                .map(Listing::Diff),
            Spot::Log => jj::history(&worktree, crate::settings::current().details.log_commits)
                .await
                .map(Listing::Log),
        };
        // Without jj, a colocated repository still reads through git.
        if !matches!(read, Err(Error::Spawn { .. })) {
            listing = Some(read);
        }
    }
    let read = match listing {
        Some(read) => read,
        None => {
            let rev = |r: &Rev| match r {
                Rev::WorkingCopy => None,
                Rev::Commit(id) => Some(id.clone()),
            };
            match &spot {
                Spot::Files(Rev::WorkingCopy) => git::changes(&worktree).await.map(Listing::Files),
                Spot::Files(Rev::Commit(id)) => {
                    git::commit_files(&worktree, id).await.map(Listing::Files)
                }
                Spot::Diff(r, c) => git::file_diff(&worktree, rev(r).as_deref(), c, diff_lines)
                    .await
                    .map(Listing::Diff),
                Spot::Log => {
                    git::history(&worktree, crate::settings::current().details.log_commits)
                        .await
                        .map(Listing::Log)
                }
            }
        }
    };
    View {
        spot,
        worktree,
        listing: read.unwrap_or_else(|e| Listing::Error(e.to_string())),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use adroit_claude::git::DiffKind;

    use adroit_model::{Session, State, Wait};

    #[test]
    fn a_session_is_told_by_its_state_how_it_runs_and_when_it_started() {
        let s = Session {
            name: "demo/fix-x".into(),
            kind: Kind::Own,
            state: State::Waiting(Wait::Permission),
            started_ms: Some(1_000_000),
            ..Default::default()
        };
        let got = facts(&s, 1_000_000 + 12 * 60_000 + 5_000);
        assert_eq!(
            got,
            [
                ("State", "waiting · permission prompt".to_string()),
                ("Runs", "in adroit".to_string()),
                ("Started", "12 min ago".to_string()),
            ]
        );
    }

    #[test]
    fn a_reader_says_so_and_an_unknown_start_is_left_out() {
        let s = Session {
            name: "demo/look".into(),
            reader: true,
            kind: Kind::Interactive,
            state: State::Idle,
            ..Default::default()
        };
        let got = facts(&s, 0);
        assert_eq!(
            got,
            [
                ("State", "idle".to_string()),
                ("Runs", "in a terminal, reading only".to_string()),
            ]
        );
    }

    #[test]
    fn how_long_ago_reads_in_the_largest_whole_unit() {
        assert_eq!(ago(30_000), "just now");
        assert_eq!(ago(90_000), "1 min ago");
        assert_eq!(ago(3 * 3_600_000 + 1), "3 h ago");
        assert_eq!(ago(2 * 86_400_000), "2 days ago");
    }

    fn limit(window: Window, percent: f64, resets_ms: Option<u64>) -> Limit {
        Limit {
            window,
            percent,
            resets_ms,
            severity: Severity::Normal,
        }
    }

    const MIN: u64 = 60_000;

    #[test]
    fn a_limit_reads_as_what_is_left_with_what_is_used_and_when_it_resets() {
        let now = 1_000 * MIN;
        let got = gauges(
            &[limit(
                Window::Session,
                3.4,
                Some(now + 4 * 60 * MIN + 13 * MIN),
            )],
            now,
        );
        assert_eq!(
            got,
            [Gauge {
                label: "Session".into(),
                used: 3,
                left: "97% left".into(),
                note: "3% used · resets in 4 h 13 min".into(),
                severity: Severity::Normal,
            }]
        );
    }

    #[test]
    fn each_window_is_named_for_its_span_and_scope() {
        let got = gauges(
            &[
                limit(Window::Session, 0.0, None),
                limit(Window::Week, 0.0, None),
                limit(Window::WeekOf("Fable".into()), 0.0, None),
                limit(Window::Other("monthly_all".into()), 0.0, None),
            ],
            0,
        );
        let labels: Vec<_> = got.iter().map(|g| g.label.as_str()).collect();
        assert_eq!(labels, ["Session", "Week", "Week · Fable", "monthly all"]);
    }

    #[test]
    fn a_passed_limit_fills_the_bar_and_leaves_nothing() {
        let got = gauges(&[limit(Window::Week, 104.0, None)], 0);
        assert_eq!(got[0].used, 100);
        assert_eq!(got[0].left, "0% left");
        assert_eq!(got[0].note, "104% used");
    }

    #[test]
    fn a_window_whose_reset_has_passed_since_it_was_asked_is_unused() {
        let got = gauges(&[limit(Window::Session, 80.0, Some(5 * MIN))], 6 * MIN);
        assert_eq!(got[0].used, 0);
        assert_eq!(got[0].left, "100% left");
        assert_eq!(got[0].note, "reset");
    }

    #[test]
    fn the_time_to_a_reset_reads_in_its_two_largest_units() {
        assert_eq!(until(30_000), "in under a minute");
        assert_eq!(until(MIN), "in 1 min");
        assert_eq!(until(59 * MIN + 59_000), "in 59 min");
        assert_eq!(until(3 * 60 * MIN), "in 3 h");
        assert_eq!(until(3 * 60 * MIN + 5 * MIN), "in 3 h 5 min");
        assert_eq!(until(24 * 60 * MIN), "in 1 day");
        assert_eq!(until(2 * 24 * 60 * MIN + 23 * 60 * MIN), "in 2 days 23 h");
    }

    #[test]
    fn sheets_step_around_in_both_directions() {
        assert_eq!(Sheet::Overview.step(1), Sheet::Changes);
        assert_eq!(Sheet::Log.step(1), Sheet::Overview);
        assert_eq!(Sheet::Overview.step(-1), Sheet::Log);
    }

    fn change(status: &str, path: &str) -> Change {
        Change {
            status: status.into(),
            path: path.into(),
            from: None,
        }
    }

    fn files(paths: &[&str]) -> Listing {
        Listing::Files(paths.iter().map(|p| change("M", p)).collect())
    }

    fn commit(id: &str, subject: &str) -> Commit {
        Commit {
            id: format!("{id}-full"),
            short: id.into(),
            subject: subject.into(),
            ..Default::default()
        }
    }

    fn log(ids: &[&str]) -> Listing {
        Listing::Log(ids.iter().map(|id| commit(id, "")).collect())
    }

    fn diff() -> Listing {
        Listing::Diff(vec![DiffLine {
            kind: DiffKind::Added,
            text: "+x".into(),
        }])
    }

    #[test]
    fn each_level_of_a_sheet_asks_for_what_it_shows() {
        let wc = Rev::WorkingCopy;
        let at = |id: &str| Rev::Commit(format!("{id}-full"));
        let mut changes = Drill::new(Sheet::Changes);
        assert_eq!(changes.spot(), Some(Spot::Files(wc.clone())));
        assert!(changes.open(&files(&["a.rs", "b.rs"])));
        assert_eq!(changes.spot(), Some(Spot::Diff(wc, change("M", "a.rs"))));

        let mut stack = Drill::new(Sheet::Log);
        assert_eq!(stack.spot(), Some(Spot::Log));
        stack.step(&log(&["x", "y"]), 1);
        assert!(stack.open(&log(&["x", "y"])));
        assert_eq!(stack.spot(), Some(Spot::Files(at("y"))));
        assert!(stack.open(&files(&["c.rs"])));
        assert_eq!(stack.spot(), Some(Spot::Diff(at("y"), change("M", "c.rs"))));

        assert_eq!(Drill::new(Sheet::Overview).spot(), None);
    }

    #[test]
    fn a_diff_opens_nothing_further_and_the_top_has_nothing_to_go_back_to() {
        let mut d = Drill::new(Sheet::Changes);
        assert!(!d.back());
        assert!(d.open(&files(&["a.rs"])));
        assert!(!d.open(&diff()));
        assert!(!d.open(&Listing::Error("gone".into())));
        assert!(d.back());
        assert!(!d.back());
    }

    #[test]
    fn going_back_selects_the_row_that_was_opened() {
        let list = log(&["x", "y", "z"]);
        let mut d = Drill::new(Sheet::Log);
        d.step(&list, 2);
        assert!(d.open(&list));
        assert!(d.open(&files(&["a.rs", "b.rs"])));
        assert!(d.back());
        assert_eq!(d.selected(&files(&["a.rs", "b.rs"])), Some(0));
        assert!(d.back());
        assert_eq!(d.selected(&list), Some(2));
    }

    #[test]
    fn the_selection_starts_on_the_first_row_and_stops_at_either_end() {
        let list = files(&["a", "b", "c"]);
        let mut d = Drill::new(Sheet::Changes);
        assert_eq!(d.selected(&list), Some(0));
        for (delta, want) in [(1, 1), (1, 2), (1, 2), (-5, 0), (-1, 0)] {
            d.step(&list, delta);
            assert_eq!(d.selected(&list), Some(want), "after {delta}");
        }
        d.last(&list);
        assert_eq!(d.selected(&list), Some(2));
        d.first(&list);
        assert_eq!(d.selected(&list), Some(0));
        assert_eq!(d.selected(&files(&[])), None);
        assert_eq!(d.selected(&diff()), None);
    }

    #[test]
    fn a_row_clicked_opens_at_once() {
        let mut d = Drill::new(Sheet::Changes);
        assert!(d.open_at(&files(&["a.rs", "b.rs"]), 1));
        assert_eq!(
            d.spot(),
            Some(Spot::Diff(Rev::WorkingCopy, change("M", "b.rs")))
        );
        assert!(!Drill::new(Sheet::Changes).open_at(&files(&["a.rs"]), 3));
    }

    #[test]
    fn a_refreshed_list_keeps_the_selected_row_or_the_place_it_was_at() {
        let mut d = Drill::new(Sheet::Changes);
        d.step(&files(&["a", "b", "c"]), 1);
        // `b` moved down: it stays selected.
        let moved = files(&["new", "a", "b", "c"]);
        assert!(!d.refreshed(&moved));
        assert_eq!(d.selected(&moved), Some(2));
        // `b` is gone: the row now where it was is selected.
        let gone = files(&["new", "a", "c"]);
        assert!(!d.refreshed(&gone));
        assert_eq!(d.selected(&gone), Some(2));
        // Rows fewer than where it was: the last one.
        let fewer = files(&["new"]);
        assert!(!d.refreshed(&fewer));
        assert_eq!(d.selected(&fewer), Some(0));
    }

    #[test]
    fn a_drilled_level_whose_item_is_gone_steps_back() {
        for (gone, why) in [
            (Listing::Error("no such revision".into()), "an error"),
            (Listing::Diff(Vec::new()), "an empty diff"),
        ] {
            let mut d = Drill::new(Sheet::Log);
            assert!(d.open(&log(&["x"])));
            assert!(d.open(&files(&["a.rs"])));
            assert!(d.refreshed(&gone), "{why}");
            assert!(matches!(d.spot(), Some(Spot::Files(_))), "{why}");
        }
    }

    #[test]
    fn an_empty_change_s_file_list_and_the_top_level_stay_put() {
        let mut d = Drill::new(Sheet::Log);
        assert!(d.open(&log(&["x"])));
        assert!(!d.refreshed(&files(&[])));
        assert!(matches!(d.spot(), Some(Spot::Files(_))));
        let mut top = Drill::new(Sheet::Changes);
        assert!(!top.refreshed(&Listing::Error("not a repository".into())));
        assert_eq!(top.spot(), Some(Spot::Files(Rev::WorkingCopy)));
    }

    #[test]
    fn the_crumbs_name_each_opened_level() {
        let mut d = Drill::new(Sheet::Log);
        assert!(d.crumbs().is_empty());
        assert!(d.open(&Listing::Log(vec![commit("wmmo", "keep the reader")])));
        let mut renamed = change("R", "new.rs");
        renamed.from = Some("old.rs".into());
        assert!(d.open(&Listing::Files(vec![renamed])));
        assert_eq!(d.crumbs(), ["wmmo keep the reader", "old.rs -> new.rs"]);
        let mut bare = Drill::new(Sheet::Log);
        assert!(bare.open(&Listing::Log(vec![commit("zzzz", "")])));
        assert_eq!(bare.crumbs(), ["zzzz (no description)"]);
    }
    fn link(kind: LinkKind, target: &str, title: Option<&str>, line: Option<u32>) -> Link {
        Link {
            kind,
            target: target.into(),
            title: title.map(str::to_string),
            line,
        }
    }

    #[test]
    fn a_link_shows_its_title_or_address_and_its_host() {
        let row = |l: &Link| shown_link(l, Path::new("/w"), Path::new("/home/me"));
        assert_eq!(
            row(&link(LinkKind::Url, "https://a.dev/x?q=1", None, None)),
            ("a.dev/x?q=1".to_string(), "a.dev".to_string())
        );
        assert_eq!(
            row(&link(
                LinkKind::Url,
                "http://a.dev/x",
                Some("The guide"),
                None
            )),
            ("The guide".to_string(), "a.dev".to_string())
        );
    }

    #[test]
    fn an_artifact_shows_its_title_or_else_its_id() {
        let row = |l: &Link| shown_link(l, Path::new("/w"), Path::new("/home/me"));
        let url = "https://claude.ai/code/artifact/abc-1";
        assert_eq!(
            row(&link(LinkKind::Artifact, url, Some("Report"), None)),
            ("Report".to_string(), "abc-1".to_string())
        );
        assert_eq!(
            row(&link(LinkKind::Artifact, url, None, None)),
            ("abc-1".to_string(), String::new())
        );
    }

    #[test]
    fn a_file_shows_its_path_from_the_worktree_or_home_and_how_it_was_touched() {
        let row = |l: &Link| shown_link(l, Path::new("/w"), Path::new("/home/me"));
        let file = |t, p, line| link(LinkKind::File(t), p, None, line);
        assert_eq!(
            row(&file(Touch::Edited, "/w/src/a.rs", None)),
            ("src/a.rs".to_string(), "edited".to_string())
        );
        assert_eq!(
            row(&file(Touch::Mentioned, "/home/me/n.md", Some(3))),
            ("~/n.md:3".to_string(), "mentioned".to_string())
        );
        assert_eq!(
            row(&file(Touch::Read, "/etc/hosts", None)),
            ("/etc/hosts".to_string(), "read".to_string())
        );
    }

    fn links(targets: &[&str]) -> Listing {
        Listing::Links(
            targets
                .iter()
                .map(|t| Entry::Link(link(LinkKind::Url, t, None, None)))
                .collect(),
        )
    }

    #[test]
    fn the_overview_s_links_are_selected_by_their_address_and_open_nothing_inside() {
        let list = links(&["https://a.dev", "https://b.dev"]);
        let mut d = Drill::new(Sheet::Overview);
        assert!(list.is_list());
        d.step(&list, 1);
        let moved = links(&["https://new.dev", "https://a.dev", "https://b.dev"]);
        assert_eq!(d.selected(&moved), Some(2));
        assert!(!d.open(&moved));
        assert_eq!(d.spot(), None);
        d.select(&moved, 0);
        assert_eq!(d.selected(&moved), Some(0));
    }
    fn heading(group: Group, count: usize, folded: bool) -> Entry {
        Entry::Heading {
            group,
            count,
            folded,
        }
    }

    #[test]
    fn the_overview_heads_each_group_and_leaves_a_folded_one_its_heading_alone() {
        let all = [
            link(
                LinkKind::Artifact,
                "https://claude.ai/artifact/a",
                None,
                None,
            ),
            link(LinkKind::Url, "https://a.dev", None, None),
            link(LinkKind::Url, "https://b.dev", None, None),
            link(LinkKind::File(Touch::Edited), "/w/a.rs", None, None),
            link(LinkKind::File(Touch::Read), "/w/b.rs", None, None),
        ];
        let open = overview(&[], &all, &HashSet::new());
        assert_eq!(
            open,
            [
                heading(Group::Artifacts, 1, false),
                Entry::Link(all[0].clone()),
                heading(Group::Links, 2, false),
                Entry::Link(all[1].clone()),
                Entry::Link(all[2].clone()),
                heading(Group::Files, 2, false),
                Entry::Link(all[3].clone()),
                Entry::Link(all[4].clone()),
            ]
        );
        let folded = overview(&[], &all, &HashSet::from([Group::Links, Group::Files]));
        assert_eq!(
            folded,
            [
                heading(Group::Artifacts, 1, false),
                Entry::Link(all[0].clone()),
                heading(Group::Links, 2, true),
                heading(Group::Files, 2, true),
            ]
        );
        assert!(overview(&[], &[], &HashSet::new()).is_empty());
    }

    #[test]
    fn a_heading_is_selected_apart_from_the_links_and_stays_selected_as_its_group_folds() {
        let all = [
            link(LinkKind::Url, "https://a.dev", None, None),
            link(LinkKind::File(Touch::Read), "/w/a.rs", None, None),
        ];
        let open = Listing::Links(overview(&[], &all, &HashSet::new()));
        assert_eq!(open.heading(Group::Files), Some(2));
        assert_eq!(open.heading(Group::Artifacts), None);
        let mut d = Drill::new(Sheet::Overview);
        d.select(&open, 2);
        let folded = Listing::Links(overview(&[], &all, &HashSet::from([Group::Links])));
        assert_eq!(d.selected(&folded), Some(1));
    }

    fn running(id: &str, name: &str, conversation: &str, from: &str) -> Session {
        Session {
            id: id.into(),
            name: name.into(),
            session_id: conversation.into(),
            started_from: from.into(),
            ..Default::default()
        }
    }

    fn peer(name: &str, sent: bool, received: bool) -> Peer {
        Peer {
            name: name.into(),
            sent,
            received,
        }
    }

    fn tied(name: &str, id: Option<&str>, ties: &[Tie]) -> Related {
        Related {
            name: name.into(),
            id: id.map(str::to_string),
            ties: ties.to_vec(),
        }
    }

    #[test]
    fn a_session_is_related_to_the_one_it_was_started_from_and_those_it_started() {
        let lead = running("1", "lead", "c-lead", "");
        let shown = running("2", "fix", "c-fix", "c-lead");
        let child = running("3", "tests", "c-tests", "c-fix");
        let other = running("4", "other", "c-other", "c-lead");
        let all = [&lead, &shown, &child, &other];
        assert_eq!(
            related(&shown, &all, &[]),
            [
                tied("lead", Some("1"), &[Tie::StartedBy]),
                tied("tests", Some("3"), &[Tie::Started]),
            ]
        );
    }

    #[test]
    fn a_peer_is_found_by_its_name_and_one_running_nowhere_is_kept_by_name() {
        let shown = running("2", "fix", "c-fix", "");
        let api = running("5", "api", "c-api", "");
        let all = [&shown, &api];
        assert_eq!(
            related(
                &shown,
                &all,
                &[peer("gone", true, false), peer("api", true, true)]
            ),
            [
                tied("gone", None, &[Tie::Sent]),
                tied("api", Some("5"), &[Tie::Sent, Tie::Received]),
            ]
        );
    }

    #[test]
    fn a_session_both_started_and_messaged_is_listed_once() {
        let shown = running("2", "fix", "c-fix", "");
        let child = running("3", "tests", "c-tests", "c-fix");
        let all = [&shown, &child];
        assert_eq!(
            related(&shown, &all, &[peer("tests", false, true)]),
            [tied("tests", Some("3"), &[Tie::Started, Tie::Received])]
        );
    }

    #[test]
    fn a_session_is_not_related_to_itself_and_one_without_a_conversation_to_none() {
        let shown = running("2", "fix", "", "");
        let stray = running("3", "stray", "c-stray", "");
        let all = [&shown, &stray];
        assert!(related(&shown, &all, &[peer("fix", true, false)]).is_empty());
    }

    #[test]
    fn the_sessions_head_the_overview_and_fold_as_a_group() {
        let r = [tied("tests", Some("3"), &[Tie::Started])];
        let l = [link(LinkKind::Url, "https://a.dev", None, None)];
        assert_eq!(
            overview(&r, &l, &HashSet::new()),
            [
                heading(Group::Sessions, 1, false),
                Entry::Session(r[0].clone()),
                heading(Group::Links, 1, false),
                Entry::Link(l[0].clone()),
            ]
        );
        assert_eq!(
            overview(&r, &[], &HashSet::from([Group::Sessions])),
            [heading(Group::Sessions, 1, true)]
        );
    }

    #[test]
    fn each_group_of_the_overview_keeps_its_newest_links_up_to_the_cap() {
        let all = [
            link(LinkKind::Url, "https://a.dev", None, None),
            link(LinkKind::Url, "https://b.dev", None, None),
            link(LinkKind::Url, "https://c.dev", None, None),
            link(LinkKind::File(Touch::Read), "/w/a.rs", None, None),
        ];
        let kept = capped(&all, 2);
        let kept: Vec<&str> = kept.iter().map(|l| l.target.as_str()).collect();
        assert_eq!(kept, ["https://a.dev", "https://b.dev", "/w/a.rs"]);
    }

    #[test]
    fn a_group_of_links_shows_its_first_few_and_a_row_for_the_rest_until_opened() {
        let all: Vec<Link> = (0..7)
            .map(|i| link(LinkKind::Url, &format!("https://{i}.dev"), None, None))
            .collect();
        let rows = overview_shown(&[], &all, &HashSet::new(), &HashSet::new(), 5);
        assert_eq!(rows.len(), 1 + 5 + 1);
        assert_eq!(
            rows[6],
            Entry::More {
                group: Group::Links,
                hidden: 2
            }
        );
        let opened = overview_shown(
            &[],
            &all,
            &HashSet::new(),
            &HashSet::from([Group::Links]),
            5,
        );
        assert_eq!(opened.len(), 1 + 7);
        // No row for none left over, nor under a folded heading.
        assert_eq!(
            overview_shown(&[], &all[..5], &HashSet::new(), &HashSet::new(), 5).len(),
            6
        );
        assert_eq!(
            overview_shown(
                &[],
                &all,
                &HashSet::from([Group::Links]),
                &HashSet::new(),
                5
            )
            .len(),
            1
        );
        // The row is selected apart from the links and the heading.
        let listing = Listing::Links(rows);
        let mut d = Drill::new(Sheet::Overview);
        d.select(&listing, 6);
        assert_eq!(d.selected(&listing), Some(6));
    }

    fn agent(call: &str, description: &str, standing: Standing) -> Agent {
        Agent {
            call: call.into(),
            id: Some(format!("a-{call}")),
            description: description.into(),
            title: None,
            kind: Some("Explore".into()),
            prompt: "Look around.".into(),
            standing,
            started_ms: Some(1_000_000),
            ended_ms: None,
            steps: Vec::new(),
        }
    }

    #[test]
    fn the_subagents_follow_the_sessions_and_fold_as_a_group() {
        let r = [tied("tests", Some("3"), &[Tie::Started])];
        let l = [link(LinkKind::Url, "https://a.dev", None, None)];
        let a = [
            agent("toolu_1", "Survey", Standing::Running),
            agent("toolu_2", "Fix", Standing::Done),
        ];
        let folded = HashSet::new();
        assert_eq!(
            with_agents(overview(&r, &l, &folded), &a, &folded),
            [
                heading(Group::Sessions, 1, false),
                Entry::Session(r[0].clone()),
                heading(Group::Agents, 2, false),
                Entry::Agent(a[0].clone()),
                Entry::Agent(a[1].clone()),
                heading(Group::Links, 1, false),
                Entry::Link(l[0].clone()),
            ]
        );
        let shut = HashSet::from([Group::Agents]);
        assert_eq!(
            with_agents(overview(&[], &l, &shut), &a, &shut),
            [
                heading(Group::Agents, 2, true),
                heading(Group::Links, 1, false),
                Entry::Link(l[0].clone()),
            ]
        );
        assert_eq!(
            with_agents(overview(&[], &l, &folded), &[], &folded),
            overview(&[], &l, &folded)
        );
    }

    #[test]
    fn a_subagent_shows_its_description_and_how_long_it_ran() {
        let start = 1_000_000;
        let mut a = agent("toolu_1", "Survey", Standing::Running);
        assert_eq!(
            shown_agent(&a, start + 3 * 60_000 + 5_000),
            ("Survey".to_string(), "running 3 min".to_string())
        );
        a.standing = Standing::Done;
        a.ended_ms = Some(start + 75 * 60_000);
        assert_eq!(shown_agent(&a, 0).1, "done in 1 h 15 min");
        a.standing = Standing::Failed;
        a.ended_ms = Some(start + 40_000);
        assert_eq!(shown_agent(&a, 0).1, "failed after 40 s");
        a.standing = Standing::Stopped;
        a.started_ms = None;
        assert_eq!(shown_agent(&a, 0).1, "stopped");
        a.description = String::new();
        assert_eq!(shown_agent(&a, 0).0, "subagent");
    }

    #[test]
    fn a_subagent_titled_by_a_later_message_goes_by_that_title_also_when_opened() {
        let mut a = agent("toolu_1", "Persistent hover panel", Standing::Running);
        a.title = Some("Rename hover to detail".into());
        assert_eq!(shown_agent(&a, 0).0, "Rename hover to detail");
        let none = HashSet::new();
        let list = Listing::Links(with_agents(Vec::new(), &[a], &none));
        let mut d = Drill::new(Sheet::Overview);
        assert!(d.open_at(&list, 1));
        assert_eq!(d.crumbs(), ["Rename hover to detail"]);
    }

    #[test]
    fn how_long_reads_in_seconds_minutes_or_hours_and_minutes() {
        assert_eq!(lasted(9_400), "9 s");
        assert_eq!(lasted(60_000), "1 min");
        assert_eq!(lasted(59 * 60_000 + 59_000), "59 min");
        assert_eq!(lasted(2 * 3_600_000), "2 h");
        assert_eq!(lasted(2 * 3_600_000 + 5 * 60_000), "2 h 5 min");
    }

    #[test]
    fn a_subagent_of_the_overview_opens_to_its_own_level_and_back_selects_it() {
        let a = [
            agent("toolu_1", "Survey", Standing::Running),
            agent("toolu_2", "Fix", Standing::Done),
        ];
        let none = HashSet::new();
        let list = Listing::Links(with_agents(Vec::new(), &a, &none));
        let mut d = Drill::new(Sheet::Overview);
        assert_eq!(d.agent(), None);
        assert!(!d.open(&list), "its heading opens nothing");
        d.step(&list, 2);
        assert!(d.open(&list));
        assert_eq!(d.agent(), Some("toolu_2"));
        assert_eq!(d.crumbs(), ["Fix"]);
        assert_eq!(d.spot(), None, "the overview is not read through git");
        assert!(d.back());
        assert_eq!(d.agent(), None);
        assert_eq!(d.selected(&list), Some(2));
        assert!(d.open_at(&list, 1));
        assert_eq!(d.agent(), Some("toolu_1"));
    }

    fn drawn(id: u64, at_ms: u64) -> Shot {
        Shot::console(id, 64, 32, at_ms, None)
    }

    fn image_file(path: &str, at_ms: Option<u64>) -> Shot {
        Shot::file(path, Touch::Read, at_ms, None)
    }

    #[test]
    fn images_run_newest_first_and_those_of_unknown_time_last() {
        let got = newest_first(vec![
            image_file("/w/old.png", Some(1)),
            image_file("/w/when.png", None),
            drawn(7, 5),
            image_file("/w/new.png", Some(9)),
        ]);
        let keys: Vec<_> = got.iter().map(|s| s.key.as_str()).collect();
        assert_eq!(
            keys,
            [
                "/w/new.png",
                drawn(7, 0).key.as_str(),
                "/w/old.png",
                "/w/when.png"
            ]
        );
    }

    #[test]
    fn an_image_shows_its_path_or_its_size_and_how_it_came_up() {
        let row = |s: &Shot| shown_shot(s, Path::new("/w"), Path::new("/home/me"));
        assert_eq!(
            row(&Shot::file("/w/out/plot.png", Touch::Edited, None, None)),
            ("out/plot.png".to_string(), "edited".to_string())
        );
        assert_eq!(
            row(&Shot::file("/home/me/a.jpg", Touch::Mentioned, None, None)),
            ("~/a.jpg".to_string(), "mentioned".to_string())
        );
        assert_eq!(
            row(&drawn(1, 0)),
            ("Image 64 × 32".to_string(), "drawn".to_string())
        );
    }

    #[test]
    fn the_images_follow_the_artifacts_and_show_their_first_few_until_opened() {
        let l = [
            link(
                LinkKind::Artifact,
                "https://claude.ai/artifact/a",
                None,
                None,
            ),
            link(LinkKind::Url, "https://a.dev", None, None),
        ];
        let shots: Vec<Shot> = (0..3).map(|i| drawn(i, 10 - i)).collect();
        let none = HashSet::new();
        let entries = overview(&[], &l, &none);
        assert_eq!(
            with_images(entries.clone(), &shots, &none, &none, 2),
            [
                heading(Group::Artifacts, 1, false),
                Entry::Link(l[0].clone()),
                heading(Group::Images, 3, false),
                Entry::Image(shots[0].clone()),
                Entry::Image(shots[1].clone()),
                Entry::More {
                    group: Group::Images,
                    hidden: 1
                },
                heading(Group::Links, 1, false),
                Entry::Link(l[1].clone()),
            ]
        );
        let opened = HashSet::from([Group::Images]);
        let shown = with_images(entries.clone(), &shots, &none, &opened, 2);
        assert_eq!(
            shown
                .iter()
                .filter(|e| matches!(e, Entry::Image(_)))
                .count(),
            3
        );
        let shut = HashSet::from([Group::Images]);
        assert_eq!(
            with_images(Vec::new(), &shots, &shut, &none, 2),
            [heading(Group::Images, 3, true)]
        );
        assert_eq!(with_images(entries.clone(), &[], &none, &none, 2), entries);
    }

    #[test]
    fn an_image_is_selected_apart_from_its_heading_and_opens_nothing_inside() {
        let shots = [drawn(1, 2), image_file("/w/a.png", Some(1))];
        let none = HashSet::new();
        let list = Listing::Links(with_images(Vec::new(), &shots, &none, &none, 5));
        assert_eq!(list.heading(Group::Images), Some(0));
        let mut d = Drill::new(Sheet::Overview);
        d.step(&list, 2);
        assert_eq!(d.selected(&list), Some(2));
        assert!(!d.open(&list));
        assert_eq!(d.spot(), None);
        let moved = Listing::Links(with_images(
            Vec::new(),
            &[drawn(9, 3), shots[0].clone(), shots[1].clone()],
            &none,
            &none,
            5,
        ));
        assert_eq!(d.selected(&moved), Some(3));
    }
}
