//! The sheets of the details panel.

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use adroit_model::links::{Kind as LinkKind, Link, Peer, Touch};
use adroit_model::subagents::{Agent, Standing};
use adroit_model::time::ago;
use adroit_model::usage::{Limit, Severity, Window};
use adroit_model::{Requester, Session, State};

use adroit_model::repository::{Change, Commit, DiffLine, Rev, Spot};
use adroit_protocol::RepositoryRead;

pub use adroit_model::Note;

mod environment;
mod items;
mod layout;
pub(crate) mod panel;
mod rows;
mod system;
mod timeline;

pub use environment::*;
pub use items::*;
pub use layout::*;
pub use panel::*;
pub use rows::*;
pub use system::*;
pub use timeline::*;

/// One line of what the overview says about a session.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Fact {
    pub label: &'static str,
    pub value: String,
    /// The colour of a value drawn as a badge, named as the sidebar's state
    /// glyphs are; `None` for plain text.
    pub tone: Option<&'static str>,
    /// Text drawn after the badge, plain: what the program says, which may
    /// run to a sentence or more and does not fit in one.
    pub note: Option<String>,
}

impl Fact {
    /// A fact drawn as plain text.
    fn plain(label: &'static str, value: String) -> Fact {
        Fact {
            label,
            value,
            tone: None,
            note: None,
        }
    }
}

/// The tone a session's `state` is drawn in.
pub fn state_tone(state: &State) -> &'static str {
    match state {
        State::Waiting(_) => "wait",
        State::Working | State::Background => "run",
        State::Done => "done",
        State::Failed => "fail",
        State::Starting | State::Idle => "idle",
    }
}

/// What the overview's head says of how the session `s` stands now, at
/// `now_ms` since the Unix epoch.
pub fn overview_facts(
    s: &Session,
    stats: &adroit_model::ConversationStats,
    last_input_ms: Option<u64>,
    now_ms: u64,
) -> Vec<Fact> {
    let mut out = Vec::new();
    if let Some(model) = &stats.model {
        out.push(Fact::plain("Model", model.clone()));
    }
    let state = match &s.state {
        State::Waiting(why) => format!("waiting · {why}"),
        State::Done => "done".into(),
        State::Failed => "failed".into(),
        other => other.word().into(),
    };
    let tone = state_tone(&s.state);
    // What a program says while it works or once it is done tends to be its
    // session's name, which the details already lead with.
    let note = match (&s.state, &s.status_message) {
        (State::Waiting(_) | State::Failed, Some(said)) => Some(said.clone()),
        _ => None,
    };
    out.push(Fact {
        label: "State",
        value: state,
        tone: Some(tone),
        note,
    });
    if let Some(started) = s.started_ms {
        out.push(Fact::plain("Started", ago(now_ms.saturating_sub(started))));
    }
    if let Some(at) = last_input_ms.max(stats.last_user_ms) {
        out.push(Fact::plain(
            "Last input",
            format!("{} ago", lasted(now_ms.saturating_sub(at))),
        ));
    }
    out
}

/// What the overview says of what the session `s` started from, as
/// `settings` name it: its profile, with its agent, and the login it runs
/// in.
fn started_from_facts(s: &Session, settings: &crate::settings::Settings) -> Vec<Fact> {
    vec![
        Fact::plain(
            "Agent setup",
            settings.started_as(s.agent, &s.profile, s.home.as_deref(), &s.profile_name),
        ),
        Fact::plain(
            "Login",
            crate::profiles::login_name(s.home.as_deref()).to_string(),
        ),
    ]
}

/// What the overview's About says of the session `s`: the totals of its
/// conversation, what it started from as `settings` name it, the
/// `workspace` and `worktree` it works in, and its conversation's ID.
pub fn about_facts(
    s: &Session,
    stats: &adroit_model::ConversationStats,
    settings: &crate::settings::Settings,
    workspace: Option<&Path>,
    worktree: Option<&Path>,
) -> Vec<Fact> {
    let mut out: Vec<Fact> = conversation_totals(stats)
        .into_iter()
        .map(|(label, value)| Fact::plain(label, value))
        .collect();
    out.extend(started_from_facts(s, settings));
    if let Some(root) = workspace {
        out.push(Fact::plain("Workspace", root.display().to_string()));
    }
    if let Some(w) = worktree {
        out.push(Fact::plain("Worktree", w.display().to_string()));
    }
    if !s.session_id.is_empty() {
        out.push(Fact::plain("Conversation", s.session_id.clone()));
    }
    out
}

/// The conversation's totals, leaving out what the provider has not
/// reported.
fn conversation_totals(stats: &adroit_model::ConversationStats) -> Vec<(&'static str, String)> {
    let mut out = Vec::new();
    if let Some(t) = stats.tokens {
        let mut used = vec![
            format!("{} in", tokens(t.input)),
            format!("{} out", tokens(t.output)),
        ];
        if t.cached > 0 {
            used.push(format!("{} cached", tokens(t.cached)));
        }
        out.push(("Tokens used", used.join(" · ")));
    }
    let mut activity = Vec::new();
    if stats.user_turns > 0 {
        activity.push(plural(stats.user_turns, "prompt", "prompts"));
    }
    if stats.tool_calls > 0 {
        activity.push(plural(stats.tool_calls, "tool", "tools"));
    }
    if !activity.is_empty() {
        out.push(("Activity", activity.join(" · ")));
    }
    out
}

/// `n` tokens in thousands or millions, to three significant figures at
/// most: the sizes differ too widely for every digit to be read.
fn tokens(n: u64) -> String {
    let scaled = |unit: f64, suffix: &str| {
        let v = n as f64 / unit;
        let shown = if v < 100.0 {
            format!("{:.1}", v)
        } else {
            format!("{:.0}", v)
        };
        format!("{}{suffix}", shown.trim_end_matches(".0"))
    };
    match n {
        0..1_000 => n.to_string(),
        // 999,999 rounds to a thousand thousands, which reads better as 1M.
        1_000..999_500 => scaled(1e3, "k"),
        _ => scaled(1e6, "M"),
    }
}

fn plural(n: u64, one: &str, many: &str) -> String {
    format!("{n} {}", if n == 1 { one } else { many })
}

/// A usage limit as the foot of the overview draws it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Gauge {
    pub label: String,
    /// How much of the bar is filled, from 0 to 100.
    pub used: u32,
    pub left: String,
    pub note: String,
    pub kind: GaugeKind,
    pub severity: Severity,
}

/// What a gauge measures.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum GaugeKind {
    /// The conversation's context.
    Context,
    /// The account's limit of the session it is in.
    Session,
    /// The account's limit of the week.
    Week,
    /// Any other account limit: one model's of the week, or one the agent
    /// names on its own.
    Other,
}

impl Gauge {
    /// Whether this is the conversation's context, not an account limit.
    pub fn context(&self) -> bool {
        self.kind == GaugeKind::Context
    }
}

/// Context utilization alongside the agent's account usage limits.
pub fn context_gauge(stats: &adroit_model::ConversationStats) -> Option<Gauge> {
    let (used, total) = stats.context.filter(|(_, total)| *total > 0)?;
    let percent = ((used as f64 / total as f64 * 100.0).round() as u32).min(100);
    Some(Gauge {
        label: "Context".into(),
        used: percent,
        left: format!("{}% left", 100 - percent),
        note: format!(
            "{percent}% used · {} / {} tokens",
            tokens(used),
            tokens(total)
        ),
        kind: GaugeKind::Context,
        severity: Severity::Normal,
    })
}

/// The gauges for `limits`, at `now_ms` since the Unix epoch.
pub fn gauges(limits: &[Limit], now_ms: u64) -> Vec<Gauge> {
    limits
        .iter()
        .map(|l| {
            let (label, kind) = match &l.window {
                Window::Session => ("Session".to_string(), GaugeKind::Session),
                Window::Week => ("Week".to_string(), GaugeKind::Week),
                Window::WeekOf(name) => (format!("Week · {name}"), GaugeKind::Other),
                Window::Other(kind) => (kind.replace('_', " "), GaugeKind::Other),
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
                kind,
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

/// What a link of Resources shows: what it is called, and a word on it.
pub fn shown_link(link: &Link, worktree: &Path, home: &Path) -> (String, String) {
    let id = || adroit_model::links::artifact_id(&link.target).unwrap_or(&link.target);
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

/// Where an image of Resources came from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Source {
    /// The console drew it: kept by its id, `width` by `height` pixels.
    Console { id: u64, width: u32, height: u32 },
    /// The session came across the file at `path`.
    File { path: String, touch: Touch },
}

/// An image of Resources.
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

/// What an image of Resources shows: what it is called, and a word on
/// how the session came by it.
pub fn shown_shot(shot: &Shot, worktree: &Path, home: &Path) -> (String, String) {
    match &shot.source {
        Source::Console { width, height, .. } => {
            (format!("Image {width} × {height}"), "drawn".to_string())
        }
        Source::File { path, touch } => shown_link(&file_link(path, *touch), worktree, home),
    }
}

/// The file at `path` as a link, as the session came across it.
fn file_link(path: &str, touch: Touch) -> Link {
    Link {
        kind: LinkKind::File(touch),
        target: path.to_string(),
        title: None,
        line: None,
    }
}

/// What a subagent shows: what it is called now, and how it
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

fn standing_word(s: Standing) -> &'static str {
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

/// What was read for a spot.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Listing {
    Files(Vec<Change>),
    Log(Vec<Commit>),
    Diff(Vec<DiffLine>),
    Error(String),
    /// The rows a sheet lists of the session shown, in the order it shows
    /// them.
    Entries(Vec<Entry>),
}

impl Listing {
    /// Whether it lists rows to select and open.
    pub fn is_list(&self) -> bool {
        matches!(
            self,
            Listing::Files(_) | Listing::Log(_) | Listing::Entries(_)
        )
    }

    /// The group whose heading row `at` is under, or is.
    pub fn group_at(&self, at: usize) -> Option<Group> {
        let Listing::Entries(entries) = self else {
            return None;
        };
        entries.get(..=at)?.iter().rev().find_map(|e| match e {
            Entry::Heading { group, .. } => Some(*group),
            _ => None,
        })
    }

    /// The entry that heads `group`.
    pub fn heading(&self, group: Group) -> Option<usize> {
        let Listing::Entries(entries) = self else {
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
            Listing::Entries(entries) => entries.iter().map(Entry::key).collect(),
            Listing::Diff(_) | Listing::Error(_) => Vec::new(),
        }
    }
}

/// A group of entries, shown under a heading of its own.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Group {
    Sessions,
    Messages,
    /// The sessions that may ask the session for work.
    Requests,
    Agents,
    Running,
    About,
    Artifacts,
    Images,
    Links,
    Pulls,
    Issues,
    Files,
    Timeline,
    /// The newest tree of each session in scope.
    Timelines,
    /// The files the working copy changes.
    Changed,
    /// The changes leading to the worktree.
    Log,
    Prompts,
    /// The live processes the session started.
    Processes,
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
            Group::Messages => "Messages",
            Group::Requests => "Requests",
            Group::Agents => "Subagents",
            Group::Running => "Running",
            Group::About => "About",
            Group::Artifacts => "Artifacts",
            Group::Images => "Images",
            Group::Links => "Links",
            Group::Pulls => "Pull requests",
            Group::Issues => "Issues",
            Group::Files => "Files",
            Group::Timeline => "Timeline",
            Group::Timelines => "Session timelines",
            Group::Changed => "Changes",
            Group::Log => "Log",
            Group::Prompts => "Prompts",
            Group::Processes => "Processes",
        }
    }
}

/// An entry: the heading of a group, or one of its rows.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Entry {
    Heading {
        group: Group,
        count: usize,
        folded: bool,
    },
    Session(Related),
    Message(Note),
    /// A session that may ask the one shown for work, which lets it or no
    /// longer lets it.
    Requester(Requester),
    Agent(Agent),
    Process(adroit_model::Process),
    Fact(Fact),
    Link(Link),
    /// A pull request or issue the conversation brought up.
    Item(ItemRow),
    Image(Shot),
    /// A tree of the timeline, heading its tasks.
    Tree(TreeCard),
    /// A task of a tree, which opens its history.
    Task(TaskLine),
    /// A session's newest tree, which shows the session.
    SessionTree(SessionTree),
    /// A file the working copy changes, which opens its diff.
    Changed(Change),
    /// A change of the log, which opens the files it changed.
    Commit(Commit),
    /// What was said to the session in a turn, which opens it whole.
    Prompt(PromptRow),
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
                Group::Messages => "\0messages",
                Group::Requests => "\0requests",
                Group::Agents => "\0agents",
                Group::Running => "\0running",
                Group::About => "\0about",
                Group::Artifacts => "\0artifacts",
                Group::Images => "\0images",
                Group::Links => "\0links",
                Group::Pulls => "\0pulls",
                Group::Issues => "\0issues",
                Group::Files => "\0files",
                Group::Timeline => "\0timeline",
                Group::Timelines => "\0timelines",
                Group::Changed => "\0changed",
                Group::Log => "\0log",
                Group::Prompts => "\0prompts",
                Group::Processes => "\0processes",
            },
            Entry::Session(r) => r.name.as_str(),
            Entry::Message(n) => n.id.as_str(),
            Entry::Requester(r) => r.code.as_str(),
            Entry::Agent(a) => a.call.as_str(),
            Entry::Process(p) => &p.key,
            Entry::Fact(f) => f.label,
            Entry::Link(l) => l.target.as_str(),
            Entry::Item(p) => p.key.as_str(),
            Entry::Image(s) => s.key.as_str(),
            Entry::Tree(card) => card.key.as_str(),
            Entry::Task(line) => line.key.as_str(),
            Entry::SessionTree(row) => row.key.as_str(),
            Entry::Changed(c) => c.path.as_str(),
            Entry::Commit(c) => c.id.as_str(),
            Entry::Prompt(p) => p.key.as_str(),
            Entry::More { group, .. } => match group {
                Group::Sessions => "\0more sessions",
                Group::Messages => "\0more messages",
                Group::Requests => "\0more requests",
                Group::Agents => "\0more agents",
                Group::Running => "\0more running",
                Group::About => "\0more about",
                Group::Artifacts => "\0more artifacts",
                Group::Images => "\0more images",
                Group::Links => "\0more links",
                Group::Pulls => "\0more pulls",
                Group::Issues => "\0more issues",
                Group::Files => "\0more files",
                Group::Timeline => "\0more timeline",
                Group::Timelines => "\0more timelines",
                Group::Changed => "\0more changed",
                Group::Log => "\0more log",
                Group::Prompts => "\0more prompts",
                Group::Processes => "\0more processes",
            },
        }
    }

    /// The group the row is in.
    pub fn group(&self) -> Group {
        match self {
            Entry::Heading { group, .. } => *group,
            Entry::Session(_) => Group::Sessions,
            Entry::Message(_) => Group::Messages,
            Entry::Requester(_) => Group::Requests,
            Entry::Agent(_) => Group::Agents,
            Entry::Process(_) => Group::Running,
            Entry::Fact(_) => Group::About,
            Entry::Link(l) => Group::of(&l.kind),
            Entry::Item(i) => match i.listing {
                adroit_model::github::Listing::Pulls => Group::Pulls,
                adroit_model::github::Listing::Issues => Group::Issues,
            },
            Entry::Image(_) => Group::Images,
            Entry::Tree(_) | Entry::Task(_) => Group::Timeline,
            Entry::SessionTree(_) => Group::Timelines,
            Entry::Changed(_) => Group::Changed,
            Entry::Commit(_) => Group::Log,
            Entry::Prompt(_) => Group::Prompts,
            Entry::More { group, .. } => *group,
        }
    }
}

/// How many of each kind run for the session.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct RunningSummary {
    pub sessions: usize,
    pub agents: usize,
    pub processes: usize,
}

impl RunningSummary {
    /// What it counts, in words: each kind that runs, by how many.
    pub fn said(&self) -> String {
        [
            (self.agents, "subagent", "subagents"),
            (self.processes, "process", "processes"),
            (self.sessions, "session", "sessions"),
        ]
        .into_iter()
        .filter(|(n, ..)| *n > 0)
        .map(|(n, one, many)| plural(n as u64, one, many))
        .collect::<Vec<_>>()
        .join(" · ")
    }
}

/// A prompt as a row lists it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PromptRow {
    /// What tells it apart from every other row.
    pub key: String,
    pub prompt: adroit_model::links::Prompt,
}

impl PromptRow {
    pub fn new(prompt: adroit_model::links::Prompt) -> Self {
        // No address, path or session name starts with a NUL.
        Self {
            key: format!("\0prompt {}", prompt.turn),
            prompt,
        }
    }

    /// The turn the row of `key` is of.
    fn turn_of(key: &str) -> Option<u64> {
        key.strip_prefix("\0prompt ")?.parse().ok()
    }

    /// Its first line.
    pub fn summary(&self) -> &str {
        self.prompt.text.lines().next().unwrap_or_default()
    }
}

/// How another session is tied to the one shown.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tie {
    /// It opened the one shown.
    OpenedBy,
    /// The one shown opened it.
    Opened,
    /// The one shown messaged it.
    Sent,
    /// It messaged the one shown.
    Received,
}

impl Tie {
    pub fn word(self) -> &'static str {
        match self {
            Tie::OpenedBy => "opened this",
            Tie::Opened => "opened",
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

/// The sessions `shown` worked with, among `sessions`: the one that opened
/// it, those it opened, then those it messaged or heard from, newest
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
    if let Some(parent) = shown.parent
        && let Some(s) = others().find(|s| s.delegation == Some(parent))
    {
        tie(&s.name, Some(&s.id), Tie::OpenedBy);
    }
    if let Some(own) = shown.delegation {
        for s in others().filter(|s| s.parent == Some(own)) {
            tie(&s.name, Some(&s.id), Tie::Opened);
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

/// The Agents sheet's rows: the `related` sessions, the `notes`, newest
/// first, and the `agents`, each group under its heading; a `folded` group
/// shows its heading alone, and the messages show their `first` and a row
/// for the rest unless the group is among those `opened`.
pub fn agents(
    related: &[Related],
    notes: &[Note],
    agents: &[Agent],
    folded: &HashSet<Group>,
    opened: &HashSet<Group>,
    first: usize,
) -> Vec<Entry> {
    let sessions = related.iter().cloned().map(Entry::Session).collect();
    let messages = notes.iter().cloned().map(Entry::Message).collect();
    let subagents = agents.iter().cloned().map(Entry::Agent).collect();
    [
        grouped(Group::Sessions, sessions, folded, opened, usize::MAX),
        grouped(Group::Messages, messages, folded, opened, first),
        grouped(Group::Agents, subagents, folded, opened, usize::MAX),
    ]
    .concat()
}

/// The Resources sheet's rows: the artifacts of `links`, the `shots`, then
/// the other addresses and the files, each group under its heading; a
/// `folded` group shows its heading alone, and one not among those `opened`
/// shows its `first` and a row for the rest.
pub fn resources(
    links: &[Link],
    shots: &[Shot],
    folded: &HashSet<Group>,
    opened: &HashSet<Group>,
    first: usize,
) -> Vec<Entry> {
    let of = |group| {
        let rows = links
            .iter()
            .filter(|l| Group::of(&l.kind) == group)
            .cloned()
            .map(Entry::Link)
            .collect();
        grouped(group, rows, folded, opened, first)
    };
    let images = shots.iter().cloned().map(Entry::Image).collect();
    [
        of(Group::Artifacts),
        grouped(Group::Images, images, folded, opened, first),
        of(Group::Links),
        of(Group::Files),
    ]
    .concat()
}

/// What works for the session shown right now, under one heading: the
/// `related` sessions that still run, the `agents` still at work and its
/// `processes`, showing their `first` and a row for the rest unless the
/// group is among those `opened`; a `folded` group shows its heading alone.
pub fn running(
    related: &[Related],
    agents: &[Agent],
    processes: &[adroit_model::Process],
    folded: &HashSet<Group>,
    opened: &HashSet<Group>,
    first: usize,
) -> Vec<Entry> {
    let rows: Vec<Entry> = related
        .iter()
        .filter(|r| r.id.is_some())
        .cloned()
        .map(Entry::Session)
        .chain(
            agents
                .iter()
                .filter(|a| a.standing == Standing::Running)
                .cloned()
                .map(Entry::Agent),
        )
        .chain(processes.iter().cloned().map(Entry::Process))
        .collect();
    grouped(Group::Running, rows, folded, opened, first)
}

/// The `facts` that change little, under their heading; a `folded` group
/// shows its heading alone.
pub fn about(facts: &[Fact], folded: &HashSet<Group>) -> Vec<Entry> {
    let rows = facts.iter().cloned().map(Entry::Fact).collect();
    grouped(Group::About, rows, folded, &HashSet::new(), usize::MAX)
}

/// `rows` under the heading of `group`, none while there are no rows: a
/// `folded` group shows its heading alone, and one not among those `opened`
/// shows its `first` rows and a row for the rest.
pub(crate) fn grouped(
    group: Group,
    rows: Vec<Entry>,
    folded: &HashSet<Group>,
    opened: &HashSet<Group>,
    first: usize,
) -> Vec<Entry> {
    if rows.is_empty() {
        return Vec::new();
    }
    let shut = folded.contains(&group);
    let mut out = vec![Entry::Heading {
        group,
        count: rows.len(),
        folded: shut,
    }];
    if !shut {
        let shown = if opened.contains(&group) {
            rows.len()
        } else {
            first.min(rows.len())
        };
        let hidden = rows.len() - shown;
        out.extend(rows.into_iter().take(shown));
        if hidden > 0 {
            out.push(Entry::More { group, hidden });
        }
    }
    out
}

/// What the `rows` of `group` come to, in a line, as the line that folds
/// them says it: how many there are, and the first of them or how they
/// stand.
pub fn said(group: Group, rows: &[Entry]) -> String {
    let n = rows
        .iter()
        .filter(|e| !matches!(e, Entry::Heading { .. } | Entry::More { .. }))
        .count();
    let first = |names: Vec<String>| {
        let mut names: Vec<String> = names
            .into_iter()
            .filter(|s| !s.is_empty())
            .take(3)
            .collect();
        names.dedup();
        names.join(", ")
    };
    let line = |what: String, rest: String| match rest.is_empty() {
        true => what,
        false => format!("{what} · {rest}"),
    };
    let link_name = |l: &Link| match (&l.kind, &l.title) {
        (LinkKind::File(_), _) => Path::new(&l.target)
            .file_name()
            .map_or_else(|| l.target.clone(), |f| f.to_string_lossy().into_owned()),
        (_, Some(title)) => title.clone(),
        (_, None) => l
            .target
            .split_once("://")
            .map_or(l.target.as_str(), |(_, r)| r)
            .split('/')
            .next()
            .unwrap_or_default()
            .to_owned(),
    };
    let names = |f: &dyn Fn(&Entry) -> Option<String>| first(rows.iter().filter_map(f).collect());
    match group {
        Group::Running => {
            let summary = RunningSummary {
                sessions: rows
                    .iter()
                    .filter(|e| matches!(e, Entry::Session(_)))
                    .count(),
                agents: rows.iter().filter(|e| matches!(e, Entry::Agent(_))).count(),
                processes: rows
                    .iter()
                    .filter(|e| matches!(e, Entry::Process(_)))
                    .count(),
            };
            summary.said()
        }
        Group::About => rows
            .iter()
            .filter_map(|e| match e {
                Entry::Fact(f) => Some(f.value.clone()),
                _ => None,
            })
            .take(2)
            .collect::<Vec<_>>()
            .join(" · "),
        Group::Agents => {
            let of = |s: Standing| {
                rows.iter()
                    .filter(|e| matches!(e, Entry::Agent(a) if a.standing == s))
                    .count()
            };
            [
                (of(Standing::Running), "running"),
                (of(Standing::Done), "done"),
                (of(Standing::Failed) + of(Standing::Stopped), "stopped"),
            ]
            .into_iter()
            .filter(|(n, _)| *n > 0)
            .map(|(n, w)| format!("{n} {w}"))
            .collect::<Vec<_>>()
            .join(" · ")
        }
        Group::Sessions => line(
            plural(n as u64, "session", "sessions"),
            names(&|e| match e {
                Entry::Session(r) => Some(r.name.clone()),
                _ => None,
            }),
        ),
        Group::Messages => line(
            plural(n as u64, "message", "messages"),
            names(&|e| match e {
                Entry::Message(m) => Some(m.other.clone()),
                _ => None,
            }),
        ),
        Group::Requests => {
            let taken = rows
                .iter()
                .filter(|e| matches!(e, Entry::Requester(r) if r.taken_up))
                .count();
            format!("Requests · {taken} of {n} as work")
        }
        Group::Processes => line(
            plural(n as u64, "process", "processes"),
            names(&|e| match e {
                Entry::Process(p) => Some(p.name.clone()),
                _ => None,
            }),
        ),
        Group::Artifacts | Group::Links | Group::Files => {
            let (one, many) = match group {
                Group::Artifacts => ("artifact", "artifacts"),
                Group::Links => ("link", "links"),
                _ => ("file", "files"),
            };
            line(
                plural(n as u64, one, many),
                names(&|e| match e {
                    Entry::Link(l) => Some(link_name(l)),
                    _ => None,
                }),
            )
        }
        Group::Images => plural(n as u64, "image", "images"),
        Group::Pulls | Group::Issues => line(
            match group {
                Group::Pulls => plural(n as u64, "pull request", "pull requests"),
                _ => plural(n as u64, "issue", "issues"),
            },
            names(&|e| match e {
                Entry::Item(p) => Some(p.reference()),
                _ => None,
            }),
        ),
        Group::Prompts => line(
            plural(n as u64, "prompt", "prompts"),
            rows.iter()
                .find_map(|e| match e {
                    Entry::Prompt(p) => Some(format!("latest: {}", p.summary())),
                    _ => None,
                })
                .unwrap_or_default(),
        ),
        Group::Changed => plural(n as u64, "file changed", "files changed"),
        Group::Log => line(
            plural(n as u64, "change", "changes"),
            rows.iter()
                .find_map(|e| match e {
                    Entry::Commit(c) => Some(format!("latest: {}", c.subject)),
                    _ => None,
                })
                .unwrap_or_default(),
        ),
        Group::Timelines => line(
            plural(n as u64, "session", "sessions"),
            names(&|e| match e {
                Entry::SessionTree(row) => Some(row.name.clone()),
                _ => None,
            }),
        ),
        Group::Timeline => rows
            .iter()
            .find_map(|e| match e {
                Entry::Tree(t) => Some(t.title.clone()),
                _ => None,
            })
            .unwrap_or_default(),
    }
}

/// What was read for a spot, and which worktree it was about.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct View {
    pub spot: Spot,
    pub worktree: PathBuf,
    pub listing: Listing,
    pub revision: Option<adroit_model::repository::Revision>,
}

impl View {
    /// What the host's `read` of the repository at `worktree` shows.
    pub fn of(worktree: PathBuf, read: RepositoryRead) -> View {
        View {
            spot: read.spot,
            worktree,
            listing: match read.listing {
                Ok(adroit_model::repository::Listing::Files(files)) => Listing::Files(files),
                Ok(adroit_model::repository::Listing::Log(commits)) => Listing::Log(commits),
                Ok(adroit_model::repository::Listing::Diff(lines)) => Listing::Diff(lines),
                Err(e) => Listing::Error(e),
            },
            revision: read.revision,
        }
    }
}

/// The change the overview says the worktree is on.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChangeLine {
    pub short: String,
    /// Its description's first line.
    pub subject: String,
    /// How many files it changes, in words.
    pub files: String,
}

/// What the overview says of the change `view` lists the files of; none
/// until its revision is known.
pub fn change_line(view: &View) -> Option<ChangeLine> {
    let Listing::Files(files) = &view.listing else {
        return None;
    };
    let revision = view.revision.as_ref()?;
    let subject = revision
        .description
        .lines()
        .next()
        .unwrap_or_default()
        .trim();
    Some(ChangeLine {
        short: revision.short.clone(),
        subject: if subject.is_empty() {
            "(no description)".into()
        } else {
            subject.to_string()
        },
        files: match files.len() {
            0 => "no changes".into(),
            n => plural(n as u64, "file", "files"),
        },
    })
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum Opened {
    Commit(Commit),
    File(Change),
    /// A subagent, by its call, and its description.
    Agent(String, String),
    /// A message of Agents, by its id, and its first line.
    Message(String, String),
    Process(String, String),
    /// A task of the timeline, by the session it is of when not the one
    /// shown, its id, and its title.
    Task(Option<String>, String, String),
    /// A prompt, by its row's key, and its first line.
    Prompt(String, String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct Cursor {
    key: String,
    at: usize,
}

/// How far into a tab the panel has gone: the rows opened, and the row
/// selected in what is shown.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Drill {
    tab: usize,
    opened: Vec<Opened>,
    cursor: Option<Cursor>,
}

impl Drill {
    pub fn new(tab: usize) -> Self {
        Drill {
            tab,
            opened: Vec::new(),
            cursor: None,
        }
    }

    /// The tab shown, by its place.
    pub fn tab(&self) -> usize {
        self.tab
    }

    /// The subagent opened, by its call.
    pub fn agent(&self) -> Option<&str> {
        match self.opened.last() {
            Some(Opened::Agent(call, _)) => Some(call),
            _ => None,
        }
    }

    pub fn message(&self) -> Option<&str> {
        match self.opened.last() {
            Some(Opened::Message(id, _)) => Some(id),
            _ => None,
        }
    }

    pub fn process(&self) -> Option<&str> {
        match self.opened.last() {
            Some(Opened::Process(key, _)) => Some(key),
            _ => None,
        }
    }

    /// The prompt opened, by its turn.
    pub fn prompt(&self) -> Option<u64> {
        match self.opened.last() {
            Some(Opened::Prompt(key, _)) => PromptRow::turn_of(key),
            _ => None,
        }
    }

    /// The task opened, by its id.
    pub fn task(&self) -> Option<&str> {
        match self.opened.last() {
            Some(Opened::Task(_, id, _)) => Some(id),
            _ => None,
        }
    }

    /// The session the task opened is of, when it is not the one shown.
    pub fn task_session(&self) -> Option<&str> {
        match self.opened.last() {
            Some(Opened::Task(session, ..)) => session.as_deref(),
            _ => None,
        }
    }

    /// Whether a row opened from the entries is shown, in place of a list.
    pub fn shows_row(&self) -> bool {
        matches!(
            self.opened.last(),
            Some(
                Opened::Agent(..)
                    | Opened::Message(..)
                    | Opened::Process(..)
                    | Opened::Task(..)
                    | Opened::Prompt(..)
            )
        )
    }

    /// What of the repository the row opened shows; none while the tab
    /// shows its components.
    pub fn spot(&self) -> Option<Spot> {
        let rev = |c: &Commit| Rev::Commit(c.id.clone());
        Some(match self.opened.as_slice() {
            [Opened::File(f), ..] => Spot::Diff(Rev::WorkingCopy, f.clone()),
            [Opened::Commit(c)] => Spot::Files(rev(c)),
            [Opened::Commit(c), Opened::File(f), ..] => Spot::Diff(rev(c), f.clone()),
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
                Opened::Message(_, summary) => summary.clone(),
                Opened::Process(_, name) => name.clone(),
                Opened::Task(_, _, title) => title.clone(),
                Opened::Prompt(_, summary) => summary.clone(),
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

    /// Select the row `key` tells apart, wherever it is listed; the first
    /// row stands in while none is.
    pub fn aim(&mut self, key: &str) {
        self.cursor = Some(Cursor {
            key: key.to_owned(),
            at: 0,
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
            Listing::Entries(entries) => match &entries[at] {
                Entry::Agent(a) => Opened::Agent(a.call.clone(), shown_agent(a, 0).0),
                Entry::Message(n) => Opened::Message(n.id.clone(), n.summary().to_owned()),
                Entry::Process(p) => Opened::Process(p.key.clone(), p.name.clone()),
                Entry::Task(t) => Opened::Task(t.session.clone(), t.id.clone(), t.title.clone()),
                Entry::Changed(c) => Opened::File(c.clone()),
                Entry::Commit(c) => Opened::Commit(c.clone()),
                Entry::Prompt(p) => Opened::Prompt(p.key.clone(), p.summary().to_owned()),
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
            Opened::Message(id, _) => id,
            Opened::Process(key, _) => key,
            Opened::Task(session, id, _) => task_key(session.as_deref(), &id),
            Opened::Prompt(key, _) => key,
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
            Listing::Files(_) | Listing::Log(_) | Listing::Entries(_) => false,
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

#[cfg(test)]
mod tests {
    use super::*;

    const OVERVIEW: usize = 0;
    const CHANGES: usize = 1;
    const LOG: usize = 2;

    use adroit_model::repository::DiffKind;

    use adroit_model::testing::{SessionBuilder, session};
    use adroit_model::{Session, State, Wait};

    fn process(pid: u32) -> adroit_model::Process {
        adroit_model::Process {
            key: format!("process-{pid}"),
            pid,
            name: format!("command-{pid}"),
            ..Default::default()
        }
    }

    #[test]
    fn the_line_that_folds_a_group_says_how_many_rows_it_has_and_the_first_of_them() {
        let file = |path: &str| {
            Entry::Link(Link {
                kind: LinkKind::File(Touch::Edited),
                target: path.into(),
                title: None,
                line: None,
            })
        };
        assert_eq!(
            said(
                Group::Files,
                &[file("/w/src/login.rs"), file("/w/README.md")]
            ),
            "2 files · login.rs, README.md"
        );
        let a = [
            agent("toolu_1", "Survey", Standing::Running),
            agent("toolu_2", "Fix", Standing::Done),
            agent("toolu_3", "Doc", Standing::Done),
        ];
        let rows: Vec<Entry> = a.iter().cloned().map(Entry::Agent).collect();
        assert_eq!(said(Group::Agents, &rows), "1 running · 2 done");
    }

    #[test]
    fn a_folded_running_says_how_many_of_each_kind_run_leaving_out_those_none_do() {
        let s = RunningSummary {
            sessions: 0,
            agents: 1,
            processes: 2,
        };
        assert_eq!(s.said(), "1 subagent · 2 processes");
    }

    #[test]
    fn running_lists_the_live_sessions_subagents_and_processes_under_one_heading() {
        let r = [
            tied("lead", Some("1"), &[Tie::OpenedBy]),
            tied("gone", None, &[Tie::Sent]),
        ];
        let a = [
            agent("toolu_1", "Survey", Standing::Running),
            agent("toolu_2", "Fix", Standing::Done),
        ];
        let p = [process(1)];
        let none = HashSet::new();
        let entries = running(&r, &a, &p, &none, &none, 5);
        assert_eq!(
            entries,
            [
                heading(Group::Running, 3, false),
                Entry::Session(r[0].clone()),
                Entry::Agent(a[0].clone()),
                Entry::Process(p[0].clone()),
            ]
        );
        let list = Listing::Entries(entries);
        assert_eq!(list.group_at(2), Some(Group::Running));
        assert_eq!(list.group_at(9), None);
        assert_eq!(
            running(&r, &a, &p, &HashSet::from([Group::Running]), &none, 5),
            [heading(Group::Running, 3, true)]
        );
        assert!(running(&r[1..], &a[1..], &[], &none, &none, 5).is_empty());
    }

    #[test]
    fn running_shows_its_first_few_and_opens_a_process_and_back_to_it() {
        let rows: Vec<_> = (1..=3).map(process).collect();
        let none = HashSet::new();
        let entries = running(&[], &[], &rows, &none, &none, 1);
        assert_eq!(
            entries,
            [
                heading(Group::Running, 3, false),
                Entry::Process(rows[0].clone()),
                Entry::More {
                    group: Group::Running,
                    hidden: 2
                },
            ]
        );
        let list = Listing::Entries(entries);
        let mut drill = Drill::new(OVERVIEW);
        assert!(drill.open_at(&list, 1));
        assert_eq!(drill.process(), Some("process-1"));
        assert!(drill.shows_row());
        assert!(drill.back());
        assert_eq!(drill.selected(&list), Some(1));
        let opened = HashSet::from([Group::Running]);
        assert_eq!(running(&[], &[], &rows, &none, &opened, 1).len(), 4);
    }

    fn fact(label: &'static str, value: &str) -> Fact {
        Fact {
            label,
            value: value.into(),
            tone: None,
            note: None,
        }
    }

    #[test]
    fn about_is_a_group_of_facts_that_opens_nothing() {
        let facts = [fact("Worktree", "/w"), fact("Conversation", "c-1")];
        assert_eq!(
            about(&facts, &HashSet::from([Group::About])),
            [heading(Group::About, 2, true)]
        );
        let list = Listing::Entries(about(&facts, &HashSet::new()));
        assert_eq!(
            list,
            Listing::Entries(vec![
                heading(Group::About, 2, false),
                Entry::Fact(facts[0].clone()),
                Entry::Fact(facts[1].clone()),
            ])
        );
        let mut d = Drill::new(OVERVIEW);
        d.select(&list, 2);
        assert_eq!(d.selected(&list), Some(2));
        assert!(!d.open(&list));
        assert_eq!(list.group_at(2), Some(Group::About));
        assert!(about(&[], &HashSet::new()).is_empty());
    }

    fn stats() -> adroit_model::ConversationStats {
        adroit_model::ConversationStats {
            model: Some("model-name".into()),
            tokens: Some(adroit_model::Tokens {
                input: 12_345_678,
                output: 45_210,
                cached: 10_100_000,
            }),
            last_user_ms: Some(1000),
            user_turns: 12,
            tool_calls: 340,
            context: Some((218_868, 1_000_000)),
        }
    }

    #[test]
    fn the_last_input_is_the_latest_user_operation() {
        let s = session("s").build();
        let input = |last: Option<u64>, stats: &adroit_model::ConversationStats, now| {
            overview_facts(&s, stats, last, now)
                .into_iter()
                .find(|f| f.label == "Last input")
                .map(|f| f.value)
        };
        assert_eq!(
            input(Some(5000), &stats(), 17000).as_deref(),
            Some("12 s ago")
        );
        assert_eq!(
            input(Some(20000), &stats(), 17000).as_deref(),
            Some("0 s ago")
        );
        let none = adroit_model::ConversationStats::default();
        assert_eq!(input(None, &none, 0), None);
    }

    #[test]
    fn conversation_totals_are_the_tokens_and_the_counts() {
        assert_eq!(
            conversation_totals(&stats()),
            [
                (
                    "Tokens used",
                    "12.3M in · 45.2k out · 10.1M cached".to_string()
                ),
                ("Activity", "12 prompts · 340 tools".to_string()),
            ]
        );
        assert!(conversation_totals(&adroit_model::ConversationStats::default()).is_empty());
    }

    #[test]
    fn context_utilization_is_a_usage_gauge_when_the_window_is_reported() {
        let gauge = context_gauge(&stats()).expect("reported window");
        assert_eq!(gauge.label, "Context");
        assert!(gauge.context());
        assert_eq!(gauge.used, 22);
        assert_eq!(gauge.left, "78% left");
        assert_eq!(gauge.note, "22% used · 219k / 1M tokens");
        let windowless = adroit_model::ConversationStats {
            context: Some((500, 0)),
            ..stats()
        };
        assert!(context_gauge(&windowless).is_none());
        assert!(context_gauge(&adroit_model::ConversationStats::default()).is_none());
    }

    #[test]
    fn the_totals_leave_out_what_was_not_reported_and_count_in_the_singular() {
        let quiet = adroit_model::ConversationStats {
            tokens: Some(adroit_model::Tokens {
                input: 900,
                output: 20,
                cached: 0,
            }),
            user_turns: 1,
            tool_calls: 1,
            ..stats()
        };
        let got = conversation_totals(&quiet);
        assert!(got.contains(&("Tokens used", "900 in · 20 out".into())));
        assert!(got.contains(&("Activity", "1 prompt · 1 tool".into())));
        let silent = adroit_model::ConversationStats {
            tokens: None,
            user_turns: 0,
            tool_calls: 0,
            ..stats()
        };
        let got = conversation_totals(&silent);
        assert!(
            !got.iter()
                .any(|(k, _)| *k == "Tokens used" || *k == "Activity")
        );
    }

    #[test]
    fn a_token_count_reads_in_thousands_or_millions() {
        assert_eq!(tokens(0), "0");
        assert_eq!(tokens(999), "999");
        assert_eq!(tokens(1_000), "1k");
        assert_eq!(tokens(1_260), "1.3k");
        assert_eq!(tokens(45_210), "45.2k");
        assert_eq!(tokens(218_868), "219k");
        assert_eq!(tokens(999_999), "1M");
        assert_eq!(tokens(1_000_000), "1M");
        assert_eq!(tokens(12_345_678), "12.3M");
        assert_eq!(tokens(150_000_000), "150M");
    }

    fn labels(facts: &[Fact]) -> Vec<&'static str> {
        facts.iter().map(|f| f.label).collect()
    }

    #[test]
    fn the_overview_leads_with_the_model_then_the_state_as_a_badge() {
        let s = session("s")
            .name("demo/fix-x")
            .working()
            .started_ms(1_000_000)
            .build();
        let now = 1_000_000 + 12 * 60_000 + 5_000;
        let got = overview_facts(&s, &stats(), Some(now - 12_000), now);
        assert_eq!(labels(&got), ["Model", "State", "Started", "Last input"]);
        assert_eq!(got[0].value, "model-name");
        assert_eq!(got[0].tone, None);
        assert_eq!(got[1].value, "working");
        assert_eq!(got[1].tone, Some("run"));
        assert_eq!(got[2].value, "12 min ago");
    }

    #[test]
    fn the_state_badge_is_coloured_by_how_the_session_stands() {
        let tone = |state: State| {
            let s = session("s").state(state).build();
            let got = overview_facts(&s, &adroit_model::ConversationStats::default(), None, 0);
            (got[0].value.clone(), got[0].tone)
        };
        assert_eq!(
            tone(State::Waiting(Wait::Permission)),
            ("waiting · permission prompt".to_string(), Some("wait"))
        );
        assert_eq!(
            tone(State::Starting),
            ("starting".to_string(), Some("idle"))
        );
        assert_eq!(tone(State::Working), ("working".to_string(), Some("run")));
        assert_eq!(
            tone(State::Background),
            ("background".to_string(), Some("run"))
        );
        assert_eq!(tone(State::Idle), ("idle".to_string(), Some("idle")));
        assert_eq!(tone(State::Done), ("done".to_string(), Some("done")));
        assert_eq!(tone(State::Failed), ("failed".to_string(), Some("fail")));
    }

    #[test]
    fn what_a_waiting_or_failed_program_says_follows_its_state_outside_the_badge() {
        let state = |state: State| {
            let s = session("s").state(state).says("Bash(rm -rf build)").build();
            let fact =
                overview_facts(&s, &adroit_model::ConversationStats::default(), None, 0).remove(0);
            (fact.value, fact.note)
        };
        let said = Some("Bash(rm -rf build)".to_string());
        assert_eq!(
            state(State::Waiting(Wait::Permission)),
            ("waiting · permission prompt".to_string(), said.clone())
        );
        assert_eq!(state(State::Failed), ("failed".to_string(), said));
        assert_eq!(state(State::Working), ("working".to_string(), None));
        assert_eq!(state(State::Done), ("done".to_string(), None));
    }

    #[test]
    fn an_unknown_model_and_start_are_left_out() {
        let s = session("s")
            .name("demo/look")
            .permissions(adroit_model::AgentPermissions::ReadOnly)
            .state(State::Idle)
            .build();
        let got = overview_facts(&s, &adroit_model::ConversationStats::default(), None, 0);
        assert_eq!(labels(&got), ["State"]);
    }

    #[test]
    fn the_overview_names_the_profile_and_the_login_a_session_started_from() {
        let settings = crate::settings::Settings::parse(
            "[[profiles]]\nid = \"claude\"\nagent = \"claude\"\n[[profiles]]\nid = \"claude-work\"\nname = \"Claude Work\"\nagent = \"claude\"\nhome = \"/w\"\n",
        )
        .unwrap();
        let shown = |s: &Session| -> Vec<(&str, String)> {
            started_from_facts(s, &settings)
                .into_iter()
                .map(|f| (f.label, f.value))
                .collect()
        };
        let work = session("s").profile("claude-work", Some("/w")).build();
        assert_eq!(
            shown(&work),
            [
                ("Agent setup", "Claude Code · Claude Work".to_string()),
                ("Login", "This profile's own".to_string())
            ]
        );
        let own = session("s").build();
        assert_eq!(
            shown(&own),
            [
                ("Agent setup", "Claude Code · claude".to_string()),
                ("Login", "Your terminal's".to_string())
            ]
        );
        // Its profile deleted since, by what it was called as it started.
        let gone = session("s")
            .profile("claude-old", Some("/o"))
            .profile_name("Claude Old")
            .build();
        assert_eq!(
            shown(&gone),
            [
                ("Agent setup", "Claude Code · Claude Old".to_string()),
                ("Login", "This profile's own".to_string())
            ]
        );
    }

    #[test]
    fn about_tells_the_totals_where_the_session_started_from_and_works_and_its_conversation() {
        let settings = crate::settings::Settings::default();
        let s = session("s").session_id("c-1").build();
        let got = about_facts(
            &s,
            &stats(),
            &settings,
            Some(Path::new("/w")),
            Some(Path::new("/w/.claude/worktrees/x")),
        );
        assert_eq!(
            labels(&got),
            [
                "Tokens used",
                "Activity",
                "Agent setup",
                "Login",
                "Workspace",
                "Worktree",
                "Conversation"
            ]
        );
        let value = |facts: &[Fact], label| {
            facts
                .iter()
                .find(|f| f.label == label)
                .map(|f| f.value.clone())
        };
        assert_eq!(value(&got, "Workspace").as_deref(), Some("/w"));
        assert_eq!(
            value(&got, "Worktree").as_deref(),
            Some("/w/.claude/worktrees/x")
        );
        assert_eq!(value(&got, "Conversation").as_deref(), Some("c-1"));
        assert!(got.iter().all(|f| f.tone.is_none()));
        // Nothing reported, no workspace and no conversation yet.
        let bare = about_facts(
            &session("s").session_id("").build(),
            &adroit_model::ConversationStats::default(),
            &settings,
            None,
            Some(Path::new("/w")),
        );
        assert_eq!(labels(&bare), ["Agent setup", "Login", "Worktree"]);
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
                kind: GaugeKind::Session,
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
    fn each_row_opened_asks_for_what_it_shows_and_the_top_for_nothing() {
        let wc = Rev::WorkingCopy;
        let at = |id: &str| Rev::Commit(format!("{id}-full"));
        let mut changes = Drill::new(CHANGES);
        assert_eq!(changes.spot(), None);
        assert!(changes.open(&files(&["a.rs", "b.rs"])));
        assert_eq!(changes.spot(), Some(Spot::Diff(wc, change("M", "a.rs"))));

        let mut stack = Drill::new(LOG);
        assert_eq!(stack.spot(), None);
        stack.step(&log(&["x", "y"]), 1);
        assert!(stack.open(&log(&["x", "y"])));
        assert_eq!(stack.spot(), Some(Spot::Files(at("y"))));
        assert!(stack.open(&files(&["c.rs"])));
        assert_eq!(stack.spot(), Some(Spot::Diff(at("y"), change("M", "c.rs"))));

        assert_eq!(Drill::new(OVERVIEW).spot(), None);
    }

    #[test]
    fn a_diff_opens_nothing_further_and_the_top_has_nothing_to_go_back_to() {
        let mut d = Drill::new(CHANGES);
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
        let mut d = Drill::new(LOG);
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
        let mut d = Drill::new(CHANGES);
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
        let mut d = Drill::new(CHANGES);
        assert!(d.open_at(&files(&["a.rs", "b.rs"]), 1));
        assert_eq!(
            d.spot(),
            Some(Spot::Diff(Rev::WorkingCopy, change("M", "b.rs")))
        );
        assert!(!Drill::new(CHANGES).open_at(&files(&["a.rs"]), 3));
    }

    #[test]
    fn a_refreshed_list_keeps_the_selected_row_or_the_place_it_was_at() {
        let mut d = Drill::new(CHANGES);
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
    fn the_change_line_tells_the_first_line_of_the_description_and_the_files_it_changes() {
        let view = |listing: Listing, description: Option<&str>| View {
            spot: Spot::Files(Rev::WorkingCopy),
            worktree: "/w".into(),
            listing,
            revision: description.map(|d| adroit_model::repository::Revision {
                id: "wmmo-full".into(),
                short: "wmmo".into(),
                description: d.into(),
            }),
        };
        let line = |l, d| change_line(&view(l, d)).map(|c| (c.short, c.subject, c.files));
        assert_eq!(
            line(files(&["a.rs", "b.rs"]), Some("core: keep it\n\nWhy.")),
            Some(("wmmo".into(), "core: keep it".into(), "2 files".into()))
        );
        assert_eq!(
            line(files(&["a.rs"]), Some("")),
            Some(("wmmo".into(), "(no description)".into(), "1 file".into()))
        );
        assert_eq!(line(files(&[]), Some("x")).unwrap().2, "no changes");
        assert_eq!(line(files(&["a.rs"]), None), None);
        assert_eq!(
            line(Listing::Error("not a repository".into()), Some("x")),
            None
        );
    }

    #[test]
    fn a_drilled_level_whose_item_is_gone_steps_back() {
        for (gone, why) in [
            (Listing::Error("no such revision".into()), "an error"),
            (Listing::Diff(Vec::new()), "an empty diff"),
        ] {
            let mut d = Drill::new(LOG);
            assert!(d.open(&log(&["x"])));
            assert!(d.open(&files(&["a.rs"])));
            assert!(d.refreshed(&gone), "{why}");
            assert!(matches!(d.spot(), Some(Spot::Files(_))), "{why}");
        }
    }

    #[test]
    fn an_empty_change_s_file_list_and_the_top_level_stay_put() {
        let mut d = Drill::new(LOG);
        assert!(d.open(&log(&["x"])));
        assert!(!d.refreshed(&files(&[])));
        assert!(matches!(d.spot(), Some(Spot::Files(_))));
        let mut top = Drill::new(CHANGES);
        assert!(!top.refreshed(&Listing::Error("not a repository".into())));
        assert_eq!(top.spot(), None);
    }

    #[test]
    fn the_crumbs_name_each_opened_level() {
        let mut d = Drill::new(LOG);
        assert!(d.crumbs().is_empty());
        assert!(d.open(&Listing::Log(vec![commit("wmmo", "keep the reader")])));
        let mut renamed = change("R", "new.rs");
        renamed.from = Some("old.rs".into());
        assert!(d.open(&Listing::Files(vec![renamed])));
        assert_eq!(d.crumbs(), ["wmmo keep the reader", "old.rs -> new.rs"]);
        let mut bare = Drill::new(LOG);
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
        Listing::Entries(
            targets
                .iter()
                .map(|t| Entry::Link(link(LinkKind::Url, t, None, None)))
                .collect(),
        )
    }

    #[test]
    fn links_are_selected_by_their_address_and_open_nothing_inside() {
        let list = links(&["https://a.dev", "https://b.dev"]);
        let mut d = Drill::new(OVERVIEW);
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
    fn resources_head_each_group_and_leave_a_folded_one_its_heading_alone() {
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
        let none = HashSet::new();
        let open = resources(&all, &[], &none, &none, usize::MAX);
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
        let shut = HashSet::from([Group::Links, Group::Files]);
        let folded = resources(&all, &[], &shut, &none, usize::MAX);
        assert_eq!(
            folded,
            [
                heading(Group::Artifacts, 1, false),
                Entry::Link(all[0].clone()),
                heading(Group::Links, 2, true),
                heading(Group::Files, 2, true),
            ]
        );
        assert!(resources(&[], &[], &none, &none, usize::MAX).is_empty());
    }

    #[test]
    fn a_heading_is_selected_apart_from_the_links_and_stays_selected_as_its_group_folds() {
        let all = [
            link(LinkKind::Url, "https://a.dev", None, None),
            link(LinkKind::File(Touch::Read), "/w/a.rs", None, None),
        ];
        let none = HashSet::new();
        let open = Listing::Entries(resources(&all, &[], &none, &none, usize::MAX));
        assert_eq!(open.heading(Group::Files), Some(2));
        assert_eq!(open.heading(Group::Artifacts), None);
        let mut d = Drill::new(OVERVIEW);
        d.select(&open, 2);
        let shut = HashSet::from([Group::Links]);
        let folded = Listing::Entries(resources(&all, &[], &shut, &none, usize::MAX));
        assert_eq!(d.selected(&folded), Some(1));
    }

    /// The record a label stands for, the same each time it is asked for.
    fn record(label: &str) -> Option<adroit_model::SessionId> {
        use std::hash::{Hash, Hasher};
        if label.is_empty() {
            return None;
        }
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        label.hash(&mut hasher);
        let hash = hasher.finish() & 0xffff_ffff_ffff;
        format!("00000000-0000-4000-8000-{hash:012x}").parse().ok()
    }

    /// A session `id` whose record is `own`, started by the one whose record is `from`.
    fn live(id: &str, name: &str, own: &str, from: &str) -> Session {
        let s = session(id).name(name);
        let s = record(own).into_iter().fold(s, SessionBuilder::delegation);
        record(from)
            .into_iter()
            .fold(s, SessionBuilder::parent)
            .build()
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
    fn a_session_is_related_to_the_one_that_opened_it_and_those_it_opened() {
        let lead = live("1", "lead", "c-lead", "");
        let shown = live("2", "fix", "c-fix", "c-lead");
        let child = live("3", "tests", "c-tests", "c-fix");
        let other = live("4", "other", "c-other", "c-lead");
        let all = [&lead, &shown, &child, &other];
        assert_eq!(
            related(&shown, &all, &[]),
            [
                tied("lead", Some("1"), &[Tie::OpenedBy]),
                tied("tests", Some("3"), &[Tie::Opened]),
            ]
        );
    }

    #[test]
    fn a_peer_is_found_by_its_name_and_one_running_nowhere_is_kept_by_name() {
        let shown = live("2", "fix", "c-fix", "");
        let api = live("5", "api", "c-api", "");
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
    fn a_session_both_opened_and_messaged_is_listed_once() {
        let shown = live("2", "fix", "c-fix", "");
        let child = live("3", "tests", "c-tests", "c-fix");
        let all = [&shown, &child];
        assert_eq!(
            related(&shown, &all, &[peer("tests", false, true)]),
            [tied("tests", Some("3"), &[Tie::Opened, Tie::Received])]
        );
    }

    #[test]
    fn a_session_is_not_related_to_itself_and_one_adroit_keeps_no_record_of_to_none() {
        let shown = live("2", "fix", "", "");
        let stray = live("3", "stray", "c-stray", "");
        let all = [&shown, &stray];
        assert!(related(&shown, &all, &[peer("fix", true, false)]).is_empty());
    }

    #[test]
    fn the_sessions_head_the_agents_sheet_and_fold_as_a_group() {
        let r = [tied("tests", Some("3"), &[Tie::Opened])];
        let none = HashSet::new();
        assert_eq!(
            agents(&r, &[], &[], &none, &none, usize::MAX),
            [
                heading(Group::Sessions, 1, false),
                Entry::Session(r[0].clone()),
            ]
        );
        let shut = HashSet::from([Group::Sessions]);
        assert_eq!(
            agents(&r, &[], &[], &shut, &none, usize::MAX),
            [heading(Group::Sessions, 1, true)]
        );
    }

    #[test]
    fn each_group_of_links_keeps_its_newest_up_to_the_cap() {
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
        let none = HashSet::new();
        let rows = resources(&all, &[], &none, &none, 5);
        assert_eq!(rows.len(), 1 + 5 + 1);
        assert_eq!(
            rows[6],
            Entry::More {
                group: Group::Links,
                hidden: 2
            }
        );
        let opened = resources(&all, &[], &none, &HashSet::from([Group::Links]), 5);
        assert_eq!(opened.len(), 1 + 7);
        // No row for none left over, nor under a folded heading.
        assert_eq!(resources(&all[..5], &[], &none, &none, 5).len(), 6);
        assert_eq!(
            resources(&all, &[], &HashSet::from([Group::Links]), &none, 5).len(),
            1
        );
        // The row is selected apart from the links and the heading.
        let listing = Listing::Entries(rows);
        let mut d = Drill::new(OVERVIEW);
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
    fn the_subagents_follow_the_sessions_and_messages_and_fold_as_a_group() {
        let r = [tied("tests", Some("3"), &[Tie::Opened])];
        let n = [Note {
            id: "n1".into(),
            outgoing: false,
            other: "tests".into(),
            body: "Done.".into(),
            asked: None,
            sent_ms: 0,
        }];
        let a = [
            agent("toolu_1", "Survey", Standing::Running),
            agent("toolu_2", "Fix", Standing::Done),
        ];
        let none = HashSet::new();
        assert_eq!(
            agents(&r, &n, &a, &none, &none, usize::MAX),
            [
                heading(Group::Sessions, 1, false),
                Entry::Session(r[0].clone()),
                heading(Group::Messages, 1, false),
                Entry::Message(n[0].clone()),
                heading(Group::Agents, 2, false),
                Entry::Agent(a[0].clone()),
                Entry::Agent(a[1].clone()),
            ]
        );
        let shut = HashSet::from([Group::Agents]);
        assert_eq!(
            agents(&[], &[], &a, &shut, &none, usize::MAX),
            [heading(Group::Agents, 2, true)]
        );
        assert_eq!(
            agents(&r, &[], &[], &none, &none, usize::MAX),
            agents(&r, &[], &[], &none, &none, 0),
            "the sessions are not capped"
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
        let list = Listing::Entries(agents(&[], &[], &[a], &none, &none, usize::MAX));
        let mut d = Drill::new(OVERVIEW);
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
    fn a_subagent_opens_to_its_own_level_and_back_selects_it() {
        let a = [
            agent("toolu_1", "Survey", Standing::Running),
            agent("toolu_2", "Fix", Standing::Done),
        ];
        let none = HashSet::new();
        let list = Listing::Entries(agents(&[], &[], &a, &none, &none, usize::MAX));
        let mut d = Drill::new(OVERVIEW);
        assert_eq!(d.agent(), None);
        assert!(!d.open(&list), "its heading opens nothing");
        assert!(!d.shows_row());
        d.step(&list, 2);
        assert!(d.open(&list));
        assert!(d.shows_row());
        assert_eq!(d.agent(), Some("toolu_2"));
        assert_eq!(d.crumbs(), ["Fix"]);
        assert_eq!(d.spot(), None, "the entries are not read through git");
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
        assert_eq!(
            resources(&l, &shots, &none, &none, 2),
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
        let shown = resources(&l, &shots, &none, &opened, 2);
        assert_eq!(
            shown
                .iter()
                .filter(|e| matches!(e, Entry::Image(_)))
                .count(),
            3
        );
        let shut = HashSet::from([Group::Images]);
        assert_eq!(
            resources(&[], &shots, &shut, &none, 2),
            [heading(Group::Images, 3, true)]
        );
        assert_eq!(
            resources(&l, &[], &none, &none, 2),
            [
                heading(Group::Artifacts, 1, false),
                Entry::Link(l[0].clone()),
                heading(Group::Links, 1, false),
                Entry::Link(l[1].clone()),
            ]
        );
    }

    #[test]
    fn an_image_is_selected_apart_from_its_heading_and_opens_nothing_inside() {
        let shots = [drawn(1, 2), image_file("/w/a.png", Some(1))];
        let none = HashSet::new();
        let list = Listing::Entries(resources(&[], &shots, &none, &none, 5));
        assert_eq!(list.heading(Group::Images), Some(0));
        let mut d = Drill::new(OVERVIEW);
        d.step(&list, 2);
        assert_eq!(d.selected(&list), Some(2));
        assert!(!d.open(&list));
        assert_eq!(d.spot(), None);
        let moved = Listing::Entries(resources(
            &[],
            &[drawn(9, 3), shots[0].clone(), shots[1].clone()],
            &none,
            &none,
            5,
        ));
        assert_eq!(d.selected(&moved), Some(3));
    }
}
