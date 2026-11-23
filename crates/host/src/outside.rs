//! `adroit session list`, `show`, `send` and `wait`: what a program outside
//! Adroit that runs as the user learns of the sessions and asks of them.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{LazyLock, Mutex};
use std::time::Duration;

use adroit_agents::ConversationReader;
use adroit_agents::links::{TURNS, Turn};
use adroit_model::time::now_ms;
use adroit_model::timeline::{Task, Timeline};
use adroit_model::{Agent, SessionCode, State, Wait};
use serde::Serialize;
use serde_json::json;

mod events;
pub(crate) mod keys;
mod steer;

pub(crate) use events::{began, follow};

use crate::command::Output;
use crate::delegation::protocol::Opener;
use crate::delegation::questions::{self, Question};
use crate::delegation::{Message, Readiness, Sender, Session as Record, app};

pub(crate) const USAGE: &str = "usage: adroit session list [--dir <path>] [--ended] [--json]\n       adroit session show <session> [--turns <n>] [--json]\n       adroit session send <session> --from <name> [--json] [-- <prompt>]\n       adroit session wait <session> [--timeout <secs>] [--json]\n       adroit session watch [--since <seq>] [--once]\n       adroit session answer <session> --question <id> (--allow | --deny [--reason <text>] | --reply <text>...) [--json] < key\n       adroit session interrupt|end <session> [--json] < key";

/// The exit status of a wait that timed out.
const TIMED_OUT: i32 = 3;
const WAIT: Duration = Duration::from_secs(60);
const WAIT_MOST: u64 = 24 * 60 * 60;
const WAIT_EVERY: Duration = Duration::from_millis(500);
/// How long a session may take to begin the turn it was started for or asked
/// for: until then, standing idle does not say that turn has ended.
const TAKES_UP: Duration = Duration::from_secs(10);
/// How long an agent's program may take to hand over a conversation.
const READING: Duration = Duration::from_secs(20);

/// The longest reply shown, the longest prompt and each reply before the
/// last, and each text of a task, in bytes.
const REPLY: usize = 24 * 1024;
const EARLIER: usize = 4 * 1024;
const TASK: usize = 512;
/// The longest input of a question shown, in bytes.
const INPUT: usize = 24 * 1024;
/// The most a session shown may take as JSON, in bytes: earlier turns and
/// then the tasks are left out to keep to it.
const SHOWN: usize = 60 * 1024;

#[derive(Debug, PartialEq, Eq)]
pub(crate) enum Asked {
    List {
        dir: Option<PathBuf>,
        ended: bool,
    },
    Show {
        session: String,
        turns: usize,
    },
    Send {
        session: String,
        from: String,
        /// None when it is to be read from standard input.
        prompt: Option<String>,
    },
    Wait {
        session: String,
        timeout: Duration,
    },
    Watch {
        since: Option<i64>,
        once: bool,
    },
    Steer {
        session: String,
        act: steer::Act,
    },
}

/// What `adroit session <args>` asks, and whether as JSON; none when `args`
/// do not say it.
pub(crate) fn parse(args: &[String]) -> Option<(Asked, bool)> {
    let (operation, rest) = args.split_first()?;
    let (mut session, mut dir, mut turns, mut from) = (None, None, None, None);
    let (mut timeout, mut prompt) = (None, None);
    let (mut since, mut once) = (None, false);
    let (mut question, mut given, mut reason) = (None, None, None);
    let mut replies = Vec::new();
    let (mut ended, mut json) = (false, false);
    let mut rest = rest.iter();
    while let Some(arg) = rest.next() {
        match (operation.as_str(), arg.as_str()) {
            (_, "--json") => json = true,
            ("list", "--dir") if dir.is_none() => dir = Some(PathBuf::from(rest.next()?)),
            ("list", "--ended") => ended = true,
            ("show", "--turns") if turns.is_none() => {
                turns = Some(rest.next()?.parse().ok().filter(|n| *n <= TURNS)?);
            }
            ("send", "--from") if from.is_none() => from = Some(rest.next()?.clone()),
            ("send", "--") => {
                let words: Vec<&str> = rest.by_ref().map(String::as_str).collect();
                prompt = Some(words.join(" "));
            }
            ("wait", "--timeout") if timeout.is_none() => {
                let secs = rest.next()?.parse().ok();
                timeout = Some(secs.filter(|s| (1..=WAIT_MOST).contains(s))?);
            }
            ("watch", "--since") if since.is_none() => {
                since = Some(rest.next()?.parse().ok().filter(|n| *n >= 0)?);
            }
            ("watch", "--once") => once = true,
            ("answer", "--question") if question.is_none() => {
                question = Some(rest.next()?.parse().ok()?);
            }
            ("answer", "--allow") if given.is_none() => given = Some(steer::Given::Allow),
            ("answer", "--deny") if given.is_none() => {
                given = Some(steer::Given::Deny { reason: None });
            }
            ("answer", "--reason") if reason.is_none() => reason = Some(rest.next()?.clone()),
            ("answer", "--reply") => replies.push(rest.next()?.clone()),
            ("show" | "send" | "wait" | "answer" | "interrupt" | "end", word)
                if !word.starts_with('-') && session.is_none() =>
            {
                session = Some(word.to_string());
            }
            _ => return None,
        }
    }
    let asked = match operation.as_str() {
        "list" => Asked::List { dir, ended },
        "show" => Asked::Show {
            session: session?,
            turns: turns.unwrap_or(1),
        },
        "send" => Asked::Send {
            session: session?,
            from: from?,
            prompt,
        },
        "wait" => Asked::Wait {
            session: session?,
            timeout: timeout.map_or(WAIT, Duration::from_secs),
        },
        "watch" => Asked::Watch { since, once },
        "answer" => Asked::Steer {
            session: session?,
            act: steer::Act::Answer {
                question: question?,
                given: answered(given, reason, replies)?,
            },
        },
        "interrupt" => Asked::Steer {
            session: session?,
            act: steer::Act::Interrupt,
        },
        "end" => Asked::Steer {
            session: session?,
            act: steer::Act::End,
        },
        _ => return None,
    };
    Some((asked, json))
}

/// The one answer the options give; none when they give none or several.
fn answered(
    given: Option<steer::Given>,
    reason: Option<String>,
    replies: Vec<String>,
) -> Option<steer::Given> {
    use steer::Given;
    match (given, reason, replies.is_empty()) {
        (Some(Given::Deny { .. }), reason, true) => Some(Given::Deny { reason }),
        (Some(given), None, true) => Some(given),
        (None, None, false) => Some(Given::Replies(replies)),
        _ => None,
    }
}

/// Whether `args` ask for one of the operations here.
pub(crate) fn takes(args: &[String]) -> bool {
    let operation = args.first().map(String::as_str);
    matches!(
        operation,
        Some("list" | "show" | "send" | "wait" | "watch" | "answer" | "interrupt" | "end")
    )
}

/// How long `adroit <args>` may wait for a session, when it is a wait.
pub(crate) fn waits_for(args: &[String]) -> Option<Duration> {
    match args.split_first() {
        Some((session, rest)) if session == "session" => match parse(rest)?.0 {
            Asked::Wait { timeout, .. } => Some(timeout),
            Asked::Watch { .. } => Some(events::PATIENCE),
            _ => None,
        },
        _ => None,
    }
}

/// A session as a program outside Adroit is told of it.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub(crate) struct Listed {
    pub id: String,
    pub code: Option<SessionCode>,
    pub name: String,
    pub agent: Agent,
    pub dir: PathBuf,
    pub state: &'static str,
    /// What a waiting session waits on the user for.
    pub waiting_for: Option<String>,
    /// What its program said with the state it last reported.
    pub message: Option<String>,
    /// The session that opened it; none for one the user, or a program of
    /// theirs, started.
    pub opened_by: Option<Opener>,
    pub started_ms: Option<u64>,
    /// When something was last said in its conversation, or typed into it.
    pub last_active_ms: Option<u64>,
}

/// How a session stands.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Standing {
    Runs(State),
    Ended,
}

impl Standing {
    /// How a session that shows `state` stands, its start recorded as
    /// `recorded`: one held at a dialog before it reports anything waits on
    /// the user.
    fn of(state: State, recorded: Option<Readiness>) -> Self {
        Self::Runs(match (state, recorded) {
            (State::Starting, Some(Readiness::Waiting)) => {
                State::Waiting(Wait::Other("a dialog as it starts".into()))
            }
            (State::Starting, Some(Readiness::Stalled)) => {
                State::Waiting(Wait::Other("its start, which stalled".into()))
            }
            (state, _) => state,
        })
    }

    fn word(&self) -> &'static str {
        match self {
            Self::Runs(state) => state.word(),
            Self::Ended => "ended",
        }
    }

    fn waiting_for(&self) -> Option<String> {
        match self {
            Self::Runs(State::Waiting(Wait::Permission)) => Some("permission".into()),
            Self::Runs(State::Waiting(Wait::Input)) => Some("input".into()),
            Self::Runs(State::Waiting(Wait::Other(on))) => Some(on.clone()),
            _ => None,
        }
    }

    /// Whether its turn has ended, with nothing waiting on the user.
    fn resting(&self) -> bool {
        matches!(self, Self::Runs(State::Idle | State::Done | State::Failed))
    }

    fn working(&self) -> bool {
        matches!(self, Self::Runs(State::Working | State::Background))
    }
}

/// What a wait knows of the turn it waits for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Awaited {
    /// How long ago the session was last started or asked for work, or was
    /// seen starting, whichever is the latest.
    asked: Option<Duration>,
    /// Whether a request still waits to be handed over.
    undelivered: bool,
    /// Whether the session was seen at work since the wait began.
    worked: bool,
    /// Whether a request was sent to it, and the turn the latest began, if
    /// that is known.
    request: Option<bool>,
    /// How long its turn has been seen over.
    rested: Duration,
}

/// Whether a session that stands so has ended its turn, or waits on the user.
fn over(standing: &Standing, awaited: Awaited) -> bool {
    match standing {
        Standing::Ended | Standing::Runs(State::Waiting(_)) => true,
        Standing::Runs(State::Idle | State::Done | State::Failed) => {
            // Work seen may have been going on before the request, which then
            // is taken up in a turn of its own once that has ended.
            let taken_up = match (awaited.worked, awaited.request) {
                (_, Some(true)) | (true, None) => true,
                (true, Some(false)) => awaited.rested >= TAKES_UP,
                (false, _) => awaited.asked.is_none_or(|ago| ago >= TAKES_UP),
            };
            taken_up && !awaited.undelivered
        }
        Standing::Runs(State::Starting | State::Working | State::Background) => false,
    }
}

/// One session, as the terminal host and Adroit's record tell of it.
struct Found {
    session: adroit_model::Session,
    standing: Standing,
    opened_by: Option<Opener>,
}

impl Found {
    fn listed(&self, spoke_ms: Option<u64>) -> Listed {
        let s = &self.session;
        Listed {
            id: s
                .delegation
                .map_or_else(|| s.id.clone(), |id| id.to_string()),
            code: s.code.clone(),
            name: s.name.clone(),
            agent: s.agent,
            dir: s.cwd.clone(),
            state: self.standing.word(),
            waiting_for: self.standing.waiting_for(),
            message: s.status_message.clone(),
            opened_by: self.opened_by.clone(),
            started_ms: s.started_ms,
            last_active_ms: [spoke_ms, s.last_input_ms, s.started_ms]
                .into_iter()
                .flatten()
                .max(),
        }
    }
}

fn opener(records: &[Record], parent: Option<adroit_model::SessionId>) -> Option<Opener> {
    let parent = parent?;
    records.iter().find(|r| r.id == parent).map(Opener::of)
}

/// A session that runs no more, as its `record` tells of it.
fn gone(record: &Record, records: &[Record]) -> Found {
    Found {
        session: adroit_model::Session {
            delegation: Some(record.id),
            code: Some(record.code.clone()),
            parent: record.parent,
            agent: record.agent,
            profile: record.profile.clone(),
            home: record.home.clone(),
            session_id: record.conversation.clone().unwrap_or_default(),
            name: record.name.clone(),
            cwd: record.cwd.clone(),
            started_ms: Some(record.started_ms),
            permissions: record.permissions.clone(),
            ..Default::default()
        },
        standing: Standing::Ended,
        opened_by: opener(records, record.parent),
    }
}

/// The session `native` is, as it stands now; none for one that is no
/// agent's, or whose program has exited.
async fn running(native: &adroit_term::Session, records: &[Record]) -> Option<Found> {
    let session = crate::sessions::agents(std::slice::from_ref(native)).next()?;
    let record = records.iter().find(|r| Some(r.id) == session.delegation);
    let explained = tokio::time::timeout(
        Duration::from_secs(3),
        crate::explain::of_native(native, &session, records),
    )
    .await;
    let state = match explained {
        Ok(Ok(explained)) => explained.state().0,
        // One whose screen cannot be read stands as its program reports.
        _ => session.state.clone(),
    };
    let reported = state == session.state;
    let standing = Standing::of(state, record.map(|r| r.readiness));
    let opened_by = opener(records, session.parent);
    let session = adroit_model::Session {
        status_message: session.status_message.clone().filter(|_| reported),
        session_id: conversation(&session, record),
        ..session
    };
    Some(Found {
        session,
        standing,
        opened_by,
    })
}

/// The conversation `session` holds. The title Codex sets may shorten its
/// id, and only the `record` then learns the whole of it.
fn conversation(session: &adroit_model::Session, record: Option<&Record>) -> String {
    let recorded = record.and_then(|r| r.conversation.clone());
    match (session.agent, recorded) {
        (Agent::Codex, Some(recorded)) => recorded,
        (_, Some(recorded)) if session.session_id.is_empty() => recorded,
        _ => session.session_id.clone(),
    }
}

async fn natives() -> Result<Vec<adroit_term::Session>, String> {
    crate::daemon::daemon()
        .sessions()
        .await
        .map_err(|e| format!("cannot reach the daemon: {e}"))
}

async fn records() -> Result<Vec<Record>, String> {
    app::sessions(&app::root()).await.map_err(|e| e.to_string())
}

/// The record of the session that runs no more and that `asked` names by its
/// id or its code, the latest of those with that code.
fn ended<'a>(records: &'a [Record], asked: &str) -> Option<&'a Record> {
    records
        .iter()
        .rev()
        .find(|r| r.id.to_string() == asked || r.code.as_str() == asked)
}

/// The session `asked` names: a running one by its id, its code or its name,
/// one that has ended by its id or its code.
async fn find(asked: &str) -> Result<Found, String> {
    let (natives, records) = (natives().await?, records().await?);
    let missing = match crate::explain::named(&natives, asked) {
        Ok(native) => {
            if let Some(found) = running(native, &records).await {
                return Ok(found);
            }
            format!("no session is named {asked}")
        }
        Err(missing) => missing,
    };
    ended(&records, asked)
        .map(|record| gone(record, &records))
        .ok_or(missing)
}

type Reader = Box<dyn ConversationReader>;
type Conversation = (Agent, PathBuf, String);

/// The reader of each conversation read, kept so that the next read takes in
/// only what was written since.
static READERS: LazyLock<Mutex<HashMap<Conversation, Reader>>> = LazyLock::new(Default::default);

/// The latest turns of `session`'s conversation, oldest first, waiting up to
/// `patience` for an agent's program that hands it over.
async fn turns(session: &adroit_model::Session, patience: Duration) -> Vec<Turn> {
    if session.session_id.is_empty() {
        return Vec::new();
    }
    let key = (
        session.agent,
        session.cwd.clone(),
        session.session_id.clone(),
    );
    let held = {
        let mut readers = READERS.lock().unwrap_or_else(|e| e.into_inner());
        readers.remove(&key)
    };
    let reader = match held {
        Some(reader) => reader,
        None => {
            let Ok(context) = crate::programs::of_session(session).await else {
                return Vec::new();
            };
            let home = crate::paths::home();
            let Some(reader) =
                adroit_agents::adapter(session.agent).transcript(&context, &home, session)
            else {
                return Vec::new();
            };
            reader
        }
    };
    let read = crate::environment::spawn_blocking(move || {
        let mut reader = reader;
        let deadline = std::time::Instant::now() + patience;
        reader.read();
        while reader.reading() && std::time::Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(50));
            reader.read();
        }
        let turns = reader.turns();
        (reader, turns)
    })
    .await;
    let Ok((reader, turns)) = read else {
        return Vec::new();
    };
    let mut readers = READERS.lock().unwrap_or_else(|e| e.into_inner());
    readers.insert(key, reader);
    turns
}

/// Forgets the conversations of sessions other than those `kept`.
fn keep_only(kept: &[Found]) {
    let mut readers = READERS.lock().unwrap_or_else(|e| e.into_inner());
    readers.retain(|(agent, cwd, id), _| {
        kept.iter().any(|f| {
            f.session.agent == *agent && f.session.cwd == *cwd && f.session.session_id == *id
        })
    });
}

async fn list(dir: Option<PathBuf>, with_ended: bool) -> Result<Vec<Listed>, String> {
    let (natives, records) = (natives().await?, records().await?);
    let mut running_now = Vec::new();
    for native in natives.iter() {
        running_now.extend(running(native, &records).await);
    }
    keep_only(&running_now);
    let mut out = Vec::new();
    for found in running_now.iter().filter(|f| within(f, dir.as_deref())) {
        let spoke = turns(&found.session, Duration::ZERO).await;
        out.push(found.listed(spoke.last().and_then(|t| t.last_ms)));
    }
    if with_ended {
        let runs = |r: &Record| {
            running_now
                .iter()
                .any(|f| f.session.delegation == Some(r.id))
        };
        out.extend(
            records
                .iter()
                .filter(|r| r.ended && !runs(r))
                .map(|r| gone(r, &records))
                .filter(|f| within(f, dir.as_deref()))
                .map(|f| f.listed(None)),
        );
    }
    Ok(out)
}

fn within(found: &Found, dir: Option<&Path>) -> bool {
    dir.is_none_or(|dir| found.session.cwd.starts_with(dir))
}

/// `text`, cut at a character to at most `most` bytes; whether it was cut.
fn cut(text: &str, most: usize) -> (&str, bool) {
    if text.len() <= most {
        return (text, false);
    }
    let end = (0..=most)
        .rev()
        .find(|end| text.is_char_boundary(*end))
        .unwrap_or_default();
    (&text[..end], true)
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
struct ShownTurn {
    #[serde(flatten)]
    turn: Turn,
    /// Whether the prompt or the reply is longer than is shown.
    truncated: bool,
}

impl ShownTurn {
    fn of(turn: &Turn, last: bool) -> Self {
        let (prompt, prompt_cut) = cut(&turn.prompt, EARLIER);
        let most = if last { REPLY } else { EARLIER };
        let reply = turn.reply.as_deref().map(|reply| cut(reply, most));
        Self {
            turn: Turn {
                prompt: prompt.into(),
                reply: reply.map(|(text, _)| text.into()),
                ..turn.clone()
            },
            truncated: prompt_cut || reply.is_some_and(|(_, cut)| cut),
        }
    }
}

/// A session with its last turns and the tasks its agent recorded.
#[derive(Debug, Clone, PartialEq, Serialize)]
struct Shown {
    #[serde(flatten)]
    listed: Listed,
    /// Oldest first.
    turns: Vec<ShownTurn>,
    timeline: Vec<Task>,
    /// Whether turns or tasks asked for were left out, or a task's text cut.
    truncated: bool,
    /// The requests from outside not handed to the session yet.
    undelivered: usize,
    /// What the session asks the user, where its agent tells of it.
    question: Option<Asks>,
}

/// What a session asks the user, as a program outside Adroit is told of it.
#[derive(Debug, Clone, PartialEq, Serialize)]
struct Asks {
    #[serde(flatten)]
    question: Question,
    /// Whether the input is too long to be shown whole, and is then the
    /// beginning of its text.
    truncated: bool,
}

impl Asks {
    fn of(question: Question) -> Self {
        let text = question.input.to_string();
        let (shown, truncated) = cut(&text, INPUT);
        let input = match truncated {
            true => json!(shown),
            false => question.input.clone(),
        };
        Self {
            question: Question { input, ..question },
            truncated,
        }
    }
}

impl Shown {
    fn new(
        listed: Listed,
        turns: &[Turn],
        asked: usize,
        tasks: &[Task],
        undelivered: usize,
        question: Option<Question>,
    ) -> Self {
        let from = turns.len().saturating_sub(asked);
        let last = turns.len().saturating_sub(1);
        let mut truncated = false;
        let mut short = |text: &str| {
            let (text, was_cut) = cut(text, TASK);
            truncated |= was_cut;
            text.to_string()
        };
        let timeline = tasks
            .iter()
            .map(|task| Task {
                title: short(&task.title),
                description: task.description.as_deref().map(&mut short),
                note: task.note.as_deref().map(&mut short),
                ..task.clone()
            })
            .collect();
        let mut shown = Self {
            listed,
            turns: turns
                .iter()
                .enumerate()
                .skip(from)
                .map(|(at, turn)| ShownTurn::of(turn, at == last))
                .collect(),
            timeline,
            truncated,
            undelivered,
            question: question.map(Asks::of),
        };
        while shown.turns.len() > 1 && shown.json().len() > SHOWN {
            shown.turns.remove(0);
            shown.truncated = true;
        }
        while !shown.timeline.is_empty() && shown.json().len() > SHOWN {
            shown.timeline.pop();
            shown.truncated = true;
        }
        if shown.json().len() > SHOWN && !shown.turns.is_empty() {
            shown.turns.clear();
            shown.truncated = true;
        }
        shown
    }

    fn json(&self) -> String {
        serde_json::to_string(self).expect("a session is serializable")
    }

    fn text(&self) -> String {
        let mut out = vec![line(&self.listed)];
        if let Some(Asks { question, .. }) = &self.question {
            out.push(format!("? {}: {}", question.tool, question.input));
        }
        for shown in &self.turns {
            out.push(format!("\n> {}", shown.turn.prompt));
            out.extend(shown.turn.reply.clone());
        }
        if !self.timeline.is_empty() {
            out.push(String::new());
        }
        out.extend(
            self.timeline
                .iter()
                .map(|t| format!("{}: {}", t.status.word(), t.title)),
        );
        out.join("\n")
    }
}

fn line(listed: &Listed) -> String {
    let code = listed.code.as_ref().map_or("-", |code| code.as_str());
    let state = match &listed.waiting_for {
        Some(on) => format!("{} ({on})", listed.state),
        None => listed.state.into(),
    };
    format!(
        "{code}\t{state}\t{}\t{}\t{}",
        listed.agent.key(),
        listed.name,
        listed.dir.display()
    )
}

/// The requests from outside the session was sent, the newest first.
async fn requests(session: &adroit_model::Session) -> Result<Vec<Message>, String> {
    let Some(id) = session.delegation else {
        return Ok(Vec::new());
    };
    let messages = app::with_store(&app::root(), move |store| store.messages_of(id))
        .await
        .map_err(|e| e.to_string())?;
    Ok(messages
        .into_iter()
        .filter(|m| matches!(m.sender, Sender::Outside(_)))
        .collect())
}

async fn show(asked: &str, turns_asked: usize) -> Result<Shown, String> {
    let found = find(asked).await?;
    let turns = turns(&found.session, READING).await;
    let entries = match found.session.code.clone() {
        Some(code) => app::timeline_at(&app::root(), code)
            .await
            .map_err(|e| e.to_string())?,
        None => Vec::new(),
    };
    let undelivered = requests(&found.session)
        .await?
        .iter()
        .filter(|m| !m.delivered)
        .count();
    let spoke = turns.last().and_then(|t| t.last_ms);
    let question = match &found.standing {
        Standing::Runs(State::Waiting(_)) => found.session.delegation.and_then(questions::of),
        _ => None,
    };
    Ok(Shown::new(
        found.listed(spoke),
        &turns,
        turns_asked,
        Timeline::replay(&entries).tasks(),
        undelivered,
        question,
    ))
}

async fn send(asked: &str, from: String, prompt: String) -> Result<(String, Listed), String> {
    let found = find(asked).await?;
    let id = found
        .session
        .delegation
        .ok_or_else(|| format!("{asked} has no record of Adroit's and cannot be asked"))?;
    let sent = app::with_store(&app::root(), move |store| {
        store.ask_from_outside(&from, id, &prompt)
    })
    .await
    .map_err(|e| e.to_string())?;
    Ok((sent.to_string(), found.listed(None)))
}

/// Whether `request` began one of `turns`, or one from before those kept.
fn begun(turns: &[Turn], request: &Message) -> bool {
    let past = turns.len() >= TURNS
        && turns
            .first()
            .and_then(|oldest| oldest.began_ms)
            .is_some_and(|at| at > request.sent_ms);
    if past {
        return true;
    }
    let body = request.body.trim();
    turns.iter().any(|turn| {
        turn.prompt.contains(body) && turn.began_ms.is_none_or(|at| at >= request.sent_ms)
    })
}

/// The session `asked` names once its turn has ended or it waits on the
/// user, or as it stands after `timeout`; whether it timed out.
async fn wait(asked: &str, timeout: Duration) -> Result<(Listed, bool), String> {
    let deadline = tokio::time::Instant::now() + timeout;
    let mut found = find(asked).await?;
    // Followed by what names it for good: its name may change meanwhile.
    let id = found.listed(None).id;
    let mut worked = false;
    let mut starting: Option<u64> = None;
    let mut rests: Option<tokio::time::Instant> = None;
    let mut taken_up = None;
    loop {
        if worked
            && found.standing.resting()
            && let Some(latest) = requests(&found.session).await?.first()
            && taken_up != Some(latest.id)
        {
            let left = deadline.saturating_duration_since(tokio::time::Instant::now());
            if begun(&turns(&found.session, READING.min(left)).await, latest) {
                taken_up = Some(latest.id);
            }
            // The turn may have begun as it was read.
            found = find(&id).await?;
        }
        worked |= found.standing.working();
        if found.standing == Standing::Runs(State::Starting) {
            starting = Some(now_ms());
        }
        let sent = requests(&found.session).await?;
        let sent_ms = sent.first().map(|m| m.sent_ms);
        let asked_ms = [found.session.started_ms, sent_ms, starting]
            .into_iter()
            .flatten()
            .max();
        rests = match found.standing.resting() {
            true => rests.or_else(|| Some(tokio::time::Instant::now())),
            false => None,
        };
        let request = sent.first().map(|latest| taken_up == Some(latest.id));
        let awaited = Awaited {
            request,
            rested: rests.map(|since| since.elapsed()).unwrap_or_default(),
            asked: asked_ms.map(|at| Duration::from_millis(now_ms().saturating_sub(at))),
            undelivered: sent.iter().any(|m| !m.delivered),
            worked,
        };
        let done = over(&found.standing, awaited);
        if done || tokio::time::Instant::now() + WAIT_EVERY > deadline {
            let spoke = turns(&found.session, Duration::ZERO).await;
            return Ok((found.listed(spoke.last().and_then(|t| t.last_ms)), !done));
        }
        tokio::time::sleep(WAIT_EVERY).await;
        found = find(&id).await?;
    }
}

async fn execute(
    asked: Asked,
    json: bool,
    cwd: &Path,
    input: Option<String>,
) -> Result<(String, i32), String> {
    let written = |value: serde_json::Value| value.to_string();
    Ok(match asked {
        Asked::List { dir, ended } => {
            let dir = dir.map(|dir| {
                let dir = cwd.join(dir);
                adroit_sys::fs::canonicalize(&dir).unwrap_or(dir)
            });
            let listed = list(dir, ended).await?;
            let text = match json {
                true => written(json!(listed)),
                false => listed.iter().map(line).collect::<Vec<_>>().join("\n"),
            };
            (text, 0)
        }
        Asked::Show { session, turns } => {
            let shown = show(&session, turns).await?;
            let text = match json {
                true => shown.json(),
                false => shown.text(),
            };
            (text, 0)
        }
        Asked::Send {
            session,
            from,
            prompt,
        } => {
            let prompt = prompt.or(input).unwrap_or_default().trim().to_string();
            let (sent, to) = send(&session, from, prompt).await?;
            let text = match json {
                true => written(json!({
                    "sent": sent,
                    "session": {"id": to.id, "code": to.code, "name": to.name},
                })),
                false => sent,
            };
            (text, 0)
        }
        Asked::Wait { session, timeout } => {
            let (listed, timed_out) = wait(&session, timeout).await?;
            let text = match json {
                true => {
                    let mut value = json!(listed);
                    value["timed_out"] = json!(timed_out);
                    written(value)
                }
                false => line(&listed),
            };
            (text, if timed_out { TIMED_OUT } else { 0 })
        }
        Asked::Steer { .. } => return Err("run apart".into()),
        Asked::Watch { since, .. } => {
            let told = events::changes(since, events::PATIENCE).await?;
            (written(json!(told)), 0)
        }
    })
}

/// `adroit session <args>` as the work host runs it for a command run in
/// `cwd` with `input` on its standard input.
pub(crate) async fn run(args: &[String], cwd: &Path, input: Option<String>) -> Output {
    let Some((asked, json)) = parse(args) else {
        return Output {
            wrote: Err(USAGE.into()),
            code: 2,
            effects_possible: false,
        };
    };
    if let Asked::Steer { session, act } = asked {
        let key = input.unwrap_or_default();
        let done = steer::run(&session, act, &key).await;
        let code = done.as_ref().err().map_or(0, |refused| refused.code);
        return Output {
            wrote: done
                .map(|done| match json {
                    true => json!({ "done": done }).to_string(),
                    false => done,
                })
                .map_err(|refused| refused.text),
            code,
            effects_possible: true,
        };
    }
    let effects_possible = matches!(asked, Asked::Send { .. });
    match execute(asked, json, cwd, input).await {
        Ok((text, code)) => Output {
            wrote: Ok(text),
            code,
            effects_possible,
        },
        Err(error) => Output {
            wrote: Err(error),
            code: 1,
            effects_possible,
        },
    }
}

/// `adroit session <args>` that acts on a session, its key read from standard
/// input: an argument or a variable of a process is read by others.
fn steer_cli(args: &[String], acting: bool) -> i32 {
    if acting {
        eprintln!("adroit: a session cannot answer or stop another");
        return 1;
    }
    match std::io::read_to_string(std::io::stdin()) {
        Ok(key) => {
            let mut ask = vec!["session".to_string()];
            ask.extend(args.iter().cloned());
            crate::work_host::run_command(ask, Some(key))
        }
        Err(e) => {
            eprintln!("adroit: cannot read the key: {e}");
            1
        }
    }
}

/// `adroit session <args>`, run in a session of Adroit's when `acting` is
/// set. Returns the process exit code.
pub(crate) fn cli(args: &[String], acting: bool) -> i32 {
    let Some((asked, _)) = parse(args) else {
        eprintln!("{USAGE}");
        return 2;
    };
    if let Asked::Steer { .. } = asked {
        return steer_cli(args, acting);
    }
    if let Asked::Watch { since, once: false } = asked {
        return events::watch(since);
    }
    let mut input = None;
    if let Asked::Send { prompt, .. } = &asked {
        if acting {
            eprintln!(
                "adroit: a session cannot ask another for work; tell it what it should know with the `send` tool"
            );
            return 1;
        }
        if prompt.is_none() {
            match std::io::read_to_string(std::io::stdin()) {
                Ok(read) => input = Some(read),
                Err(e) => {
                    eprintln!("adroit: cannot read the prompt: {e}");
                    return 1;
                }
            }
        }
    }
    let mut ask = vec!["session".to_string()];
    ask.extend(args.iter().cloned());
    crate::work_host::run_command(ask, input)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::record;
    use adroit_model::timeline::Status;

    fn args(words: &[&str]) -> Vec<String> {
        words.iter().map(|w| w.to_string()).collect()
    }

    fn asked(words: &[&str]) -> Option<Asked> {
        parse(&args(words)).map(|(asked, _)| asked)
    }

    #[test]
    fn each_operation_takes_its_own_options_and_no_other() {
        assert_eq!(
            parse(&args(&["list", "--dir", "/src", "--ended", "--json"])),
            Some((
                Asked::List {
                    dir: Some("/src".into()),
                    ended: true
                },
                true
            ))
        );
        assert_eq!(
            asked(&["show", "wwdc", "--turns", "3"]),
            Some(Asked::Show {
                session: "wwdc".into(),
                turns: 3
            })
        );
        assert_eq!(
            asked(&["send", "wwdc", "--from", "Famulen", "--", "go", "--json"]),
            Some(Asked::Send {
                session: "wwdc".into(),
                from: "Famulen".into(),
                prompt: Some("go --json".into())
            })
        );
        assert_eq!(
            asked(&["wait", "wwdc"]),
            Some(Asked::Wait {
                session: "wwdc".into(),
                timeout: WAIT
            })
        );
        for wrong in [
            &["list", "wwdc"][..],
            &["list", "--turns", "1"],
            &["show"],
            &["show", "a", "b"],
            &["show", "a", "--turns", "21"],
            &["send", "wwdc", "--", "go"],
            &["send", "--from", "Famulen", "--", "go"],
            &["wait", "wwdc", "--timeout", "0"],
            &["wait", "wwdc", "--from", "x"],
            &["watch", "wwdc"],
            &["watch", "--since", "-1"],
            &["answer", "wwdc"],
            &["interrupt"],
            &["end", "wwdc", "--allow"],
        ] {
            assert_eq!(asked(wrong), None, "{wrong:?}");
        }
    }

    #[test]
    fn a_watch_goes_on_from_the_change_named_until_stopped_unless_asked_once() {
        assert_eq!(
            asked(&["watch", "--since", "4"]),
            Some(Asked::Watch {
                since: Some(4),
                once: false
            })
        );
        assert_eq!(
            asked(&["watch", "--once"]),
            Some(Asked::Watch {
                since: None,
                once: true
            })
        );
    }

    #[test]
    fn an_answer_names_its_question_and_gives_one_answer() {
        let id = adroit_model::MessageId::new();
        let text = id.to_string();
        let given = |rest: &[&str]| {
            let words = [&["answer", "wwdc", "--question", &text][..], rest].concat();
            match asked(&words) {
                Some(Asked::Steer {
                    act: steer::Act::Answer { question, given },
                    ..
                }) if question == id => Some(given),
                _ => None,
            }
        };
        use steer::Given;
        assert_eq!(given(&["--allow"]), Some(Given::Allow));
        assert_eq!(
            given(&["--deny", "--reason", "not now"]),
            Some(Given::Deny {
                reason: Some("not now".into())
            })
        );
        assert_eq!(
            given(&["--reply", "Red", "--reply", "--big"]),
            Some(Given::Replies(vec!["Red".into(), "--big".into()]))
        );
        for wrong in [
            &[][..],
            &["--allow", "--deny"],
            &["--allow", "--reason", "x"],
            &["--allow", "--reply", "x"],
            &["--deny", "--reply", "x"],
            &["--reason", "x"],
        ] {
            assert_eq!(given(wrong), None, "{wrong:?}");
        }
        assert_eq!(asked(&["answer", "wwdc", "--allow"]), None);
        assert_eq!(
            asked(&["answer", "wwdc", "--question", "x", "--allow"]),
            None
        );
    }

    #[test]
    fn a_question_is_shown_whole_or_says_it_was_cut() {
        let question = |command: &str| Question {
            id: adroit_model::MessageId::new(),
            kind: questions::Kind::Permission,
            tool: "Bash".into(),
            input: json!({"command": command}),
            dir: Some("/repo".into()),
            asked_ms: 1,
        };
        let whole = Asks::of(question("ls"));
        assert_eq!(whole.question.input["command"], "ls");
        assert!(!whole.truncated);
        let long = Asks::of(question(&"x".repeat(INPUT)));
        assert!(long.truncated);
        assert_eq!(long.question.input.as_str().unwrap().len(), INPUT);
        let turns: Vec<Turn> = (1..=3).map(|n| turn(n, &"\n".repeat(REPLY))).collect();
        let shown = Shown::new(
            listed(),
            &turns,
            3,
            &[],
            0,
            Some(question(&"\"".repeat(INPUT))),
        );
        assert!(shown.json().len() <= SHOWN, "{}", shown.json().len());
    }

    #[test]
    fn only_a_wait_waits() {
        assert_eq!(
            waits_for(&args(&["session", "wait", "a", "--timeout", "5"])),
            Some(Duration::from_secs(5))
        );
        assert_eq!(waits_for(&args(&["session", "show", "a"])), None);
        assert_eq!(waits_for(&args(&["terminal", "wait", "a"])), None);
    }

    const SETTLED: Awaited = Awaited {
        asked: None,
        undelivered: false,
        worked: false,
        request: None,
        rested: Duration::ZERO,
    };

    #[test]
    fn work_seen_is_a_request_s_only_once_the_turn_it_began_is_known_or_none_follows() {
        let idle = Standing::Runs(State::Idle);
        let older_work = Awaited {
            asked: Some(TAKES_UP * 2),
            worked: true,
            request: Some(false),
            ..SETTLED
        };
        assert!(!over(&idle, older_work));
        assert!(over(
            &idle,
            Awaited {
                request: Some(true),
                ..older_work
            }
        ));
        assert!(over(
            &idle,
            Awaited {
                rested: TAKES_UP,
                ..older_work
            }
        ));
    }

    #[test]
    fn a_request_began_the_turn_whose_prompt_holds_it_and_none_from_before_it() {
        let mut request = crate::testing::message(
            adroit_model::SessionId::new(),
            adroit_model::SessionId::new(),
        )
        .body("go on\n")
        .build();
        request.sent_ms = 10;
        let turn = |prompt: &str, began_ms| Turn {
            prompt: prompt.into(),
            began_ms: Some(began_ms),
            ..Default::default()
        };
        let headed = "[request] from Famulen:\n\ngo on";
        assert!(begun(&[turn(headed, 11), turn("other", 12)], &request));
        assert!(!begun(&[turn(headed, 9), turn("other", 12)], &request));
        // Its turn has left what is kept once every turn kept began after it.
        let later = |n| (0..n).map(|_| turn("other", 12)).collect::<Vec<_>>();
        assert!(begun(&later(TURNS), &request));
        assert!(!begun(&later(TURNS - 1), &request));
    }

    #[test]
    fn a_wait_ends_with_the_turn_with_the_session_or_when_it_waits_on_the_user() {
        for (state, ends) in [
            (State::Idle, true),
            (State::Done, true),
            (State::Failed, true),
            (State::Waiting(Wait::Permission), true),
            (State::Working, false),
            (State::Background, false),
            (State::Starting, false),
        ] {
            assert_eq!(
                over(&Standing::Runs(state.clone()), SETTLED),
                ends,
                "{state:?}"
            );
        }
        assert!(over(&Standing::Ended, SETTLED));
    }

    #[test]
    fn a_session_just_asked_is_waited_for_until_it_takes_the_work_up() {
        let idle = Standing::Runs(State::Idle);
        let just = Awaited {
            asked: Some(Duration::from_secs(1)),
            ..SETTLED
        };
        assert!(!over(&idle, just));
        assert!(over(
            &idle,
            Awaited {
                worked: true,
                ..just
            }
        ));
        let long_ago = Awaited {
            asked: Some(TAKES_UP),
            ..just
        };
        assert!(over(&idle, long_ago));
        let undelivered = Awaited {
            undelivered: true,
            ..long_ago
        };
        assert!(!over(&idle, undelivered));
        // What holds it up is the user's to answer.
        let waiting = Standing::Runs(State::Waiting(Wait::Input));
        assert!(over(&waiting, undelivered));
    }

    #[test]
    fn a_session_starting_has_not_worked_yet() {
        assert!(!Standing::Runs(State::Starting).working());
        assert!(Standing::Runs(State::Working).working());
        assert!(Standing::Runs(State::Background).working());
    }

    #[test]
    fn a_session_held_as_it_starts_waits_on_the_user() {
        let held = Standing::of(State::Starting, Some(Readiness::Waiting));
        assert_eq!(held.word(), "waiting");
        assert!(held.waiting_for().is_some());
        let stalled = Standing::of(State::Starting, Some(Readiness::Stalled));
        assert_eq!(stalled.word(), "waiting");
        let starting = Standing::of(State::Starting, Some(Readiness::Starting));
        assert_eq!(
            (starting.word(), starting.waiting_for()),
            ("starting", None)
        );
        let asks = Standing::of(State::Waiting(Wait::Permission), Some(Readiness::Ready));
        assert_eq!(asks.waiting_for().as_deref(), Some("permission"));
    }

    #[test]
    fn a_codex_conversation_is_the_one_recorded_and_any_other_the_one_the_session_holds() {
        let held = |agent, id: &str| {
            conversation(
                &adroit_model::Session {
                    agent,
                    session_id: id.into(),
                    ..Default::default()
                },
                Some(&record("a").conversation("recorded").build()),
            )
        };
        assert_eq!(held(Agent::Codex, ""), "recorded");
        assert_eq!(held(Agent::Codex, "held"), "recorded");
        assert_eq!(held(Agent::Claude, "held"), "held");
        assert_eq!(held(Agent::Claude, ""), "recorded");
    }

    #[test]
    fn a_session_that_ended_is_named_by_its_id_or_its_code_the_latest_first() {
        let (mut first, mut second) = (record("a").ended().build(), record("a").ended().build());
        first.code = "k7f3".parse().unwrap();
        second.code = first.code.clone();
        let records = [first.clone(), second.clone()];
        assert_eq!(ended(&records, "k7f3").unwrap().id, second.id);
        assert_eq!(ended(&records, &first.id.to_string()).unwrap().id, first.id);
        assert!(ended(&records, "a").is_none());
        assert_eq!(gone(&first, &records).listed(None).state, "ended");
    }

    fn turn(n: u64, reply: &str) -> Turn {
        Turn {
            turn: n,
            prompt: "p".into(),
            reply: Some(reply.into()),
            ..Default::default()
        }
    }

    fn task(note: &str) -> Task {
        Task {
            id: "t".into(),
            parent: None,
            title: "title".into(),
            status: Status::Pending,
            weight: 1.0,
            description: None,
            note: Some(note.into()),
        }
    }

    fn listed() -> Listed {
        let record = record("a").build();
        gone(&record, std::slice::from_ref(&record)).listed(None)
    }

    #[test]
    fn the_last_turns_asked_for_are_shown_oldest_first() {
        let turns = [turn(1, "a"), turn(2, "b"), turn(3, "c")];
        let shown = Shown::new(listed(), &turns, 2, &[task("ok")], 0, None);
        let replies: Vec<_> = shown
            .turns
            .iter()
            .map(|t| t.turn.reply.as_deref().unwrap())
            .collect();
        assert_eq!(replies, ["b", "c"]);
        assert!(!shown.truncated && !shown.turns[0].truncated);
        assert!(
            Shown::new(listed(), &turns, 0, &[], 0, None)
                .turns
                .is_empty()
        );
    }

    #[test]
    fn what_is_shown_keeps_to_a_size_and_says_what_was_cut() {
        let long = "あ".repeat(REPLY);
        let turns: Vec<Turn> = (1..=TURNS as u64).map(|n| turn(n, &long)).collect();
        let tasks: Vec<Task> = (0..200).map(|_| task(&long)).collect();
        let shown = Shown::new(listed(), &turns, TURNS, &tasks, 0, None);
        assert!(shown.json().len() <= SHOWN);
        assert!(shown.truncated);
        let last = shown.turns.last().unwrap();
        assert_eq!(last.turn.turn, TURNS as u64);
        assert!(last.truncated);
        assert!(last.turn.reply.as_ref().unwrap().len() <= REPLY);
    }
}
