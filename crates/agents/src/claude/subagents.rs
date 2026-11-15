//! The subagents a session runs, read from the transcript Claude Code writes
//! of its conversation and from the one it writes of each subagent.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use serde_json::Value;

use crate::time::epoch_ms;

pub use adroit_model::subagents::*;

/// The most subagents that ended kept: the latest to end.
pub const KEPT_ENDED: usize = 5;

/// The most steps of a subagent kept: the latest.
pub const STEPS: usize = 20;

/// The most of a subagent's transcript read for its steps, from its end.
const TAIL: u64 = 256 * 1024;

/// The most of a step's text kept.
const STEP_TEXT: usize = 4096;

#[derive(Debug, Clone)]
struct Held {
    agent: Agent,
    /// The order it last ended in, among the others.
    ended: u64,
}

/// What the lines of a session's transcript tell of its subagents, taken in
/// a line at a time.
#[derive(Debug, Clone, Default)]
pub struct Collector {
    /// In the order they were started.
    held: Vec<Held>,
    endings: u64,
    /// The summary of each message to a subagent not yet delivered, with
    /// the subagent's id, by the tool call's id.
    sending: HashMap<String, (String, String)>,
}

impl Collector {
    /// Take in one line of the transcript, parsed.
    pub fn feed(&mut self, v: &Value) {
        let at = v["timestamp"].as_str().and_then(epoch_ms);
        match v["type"].as_str() {
            Some("assistant") => {
                for item in items(v) {
                    if item["type"].as_str() == Some("tool_use") {
                        self.started(item, at);
                        self.sending(item);
                    }
                }
            }
            Some("user") => {
                let outcome = &v["toolUseResult"];
                if let Some(id) = outcome["resumedAgentId"].as_str() {
                    self.resumed(id);
                }
                match &v["message"]["content"] {
                    Value::String(s) => self.notified(s, at),
                    Value::Array(parts) => {
                        for item in parts {
                            if item["type"].as_str() == Some("tool_result") {
                                self.delivered(item, outcome);
                                self.answered(item, outcome, at);
                            }
                        }
                    }
                    _ => {}
                }
            }
            Some("queue-operation") if v["operation"].as_str() == Some("enqueue") => {
                if let Some(s) = v["content"].as_str() {
                    self.notified(s, at);
                }
            }
            Some("attachment") if v["attachment"]["type"].as_str() == Some("queued_command") => {
                if let Some(s) = v["attachment"]["prompt"].as_str() {
                    self.notified(s, at);
                }
            }
            _ => {}
        }
    }

    /// The subagents running, the latest started first, then the
    /// [`KEPT_ENDED`] latest to end, the latest first.
    pub fn agents(&self) -> Vec<Agent> {
        let running = self
            .held
            .iter()
            .rev()
            .filter(|h| h.agent.standing == Standing::Running);
        let mut ended: Vec<&Held> = self
            .held
            .iter()
            .filter(|h| h.agent.standing != Standing::Running)
            .collect();
        ended.sort_by_key(|h| std::cmp::Reverse(h.ended));
        running
            .chain(ended.into_iter().take(KEPT_ENDED))
            .map(|h| h.agent.clone())
            .collect()
    }

    fn started(&mut self, item: &Value, at: Option<u64>) {
        if !matches!(item["name"].as_str(), Some("Agent" | "Task")) {
            return;
        }
        let (Some(call), input) = (item["id"].as_str(), &item["input"]) else {
            return;
        };
        let text = |k: &str| input[k].as_str().map(str::to_string);
        self.held.push(Held {
            agent: Agent {
                call: call.to_string(),
                id: None,
                description: text("description").unwrap_or_default(),
                title: None,
                kind: text("subagent_type"),
                prompt: text("prompt").unwrap_or_default(),
                standing: Standing::Running,
                started_ms: at,
                ended_ms: None,
                steps: Vec::new(),
            },
            ended: 0,
        });
    }

    /// The tool call that started a subagent came back: launched to run on,
    /// or done with what it ran to.
    fn answered(&mut self, item: &Value, outcome: &Value, at: Option<u64>) {
        let Some(call) = item["tool_use_id"].as_str() else {
            return;
        };
        let Some(h) = self.held.iter_mut().find(|h| h.agent.call == call) else {
            return;
        };
        if let Some(id) = outcome["agentId"].as_str() {
            h.agent.id = Some(id.to_string());
        }
        if outcome["status"].as_str() == Some("async_launched") {
            return;
        }
        let standing = if item["is_error"].as_bool() == Some(true) {
            Standing::Failed
        } else {
            Standing::Done
        };
        self.endings += 1;
        end(h, standing, at, self.endings);
    }

    fn sending(&mut self, item: &Value) {
        let input = &item["input"];
        if item["name"].as_str() != Some("SendMessage") {
            return;
        }
        if let (Some(call), Some(to), Some(summary)) = (
            item["id"].as_str(),
            input["to"].as_str(),
            input["summary"]
                .as_str()
                .map(str::trim)
                .filter(|s| !s.is_empty()),
        ) {
            self.sending
                .insert(call.to_string(), (to.to_string(), summary.to_string()));
        }
    }

    /// A message to a subagent was delivered: its summary titles it.
    fn delivered(&mut self, item: &Value, outcome: &Value) {
        let Some((to, summary)) = item["tool_use_id"]
            .as_str()
            .and_then(|call| self.sending.remove(call))
        else {
            return;
        };
        if outcome["success"].as_bool() != Some(true) {
            return;
        }
        if let Some(h) = self
            .held
            .iter_mut()
            .find(|h| h.agent.id.as_deref() == Some(to.as_str()))
        {
            h.agent.title = Some(summary);
        }
    }

    fn resumed(&mut self, id: &str) {
        if let Some(h) = self
            .held
            .iter_mut()
            .find(|h| h.agent.id.as_deref() == Some(id))
        {
            h.agent.standing = Standing::Running;
            h.agent.ended_ms = None;
        }
    }

    /// Claude Code told the session a task it runs in the background ended.
    /// The same notice is written more than once, so one about a subagent
    /// not running changes nothing.
    fn notified(&mut self, text: &str, at: Option<u64>) {
        let Some(notice) = between(text, "<task-notification>", "</task-notification>") else {
            return;
        };
        let call = between(notice, "<tool-use-id>", "</tool-use-id>");
        let task = between(notice, "<task-id>", "</task-id>");
        let standing = match between(notice, "<status>", "</status>") {
            Some("completed") => Standing::Done,
            Some("failed") => Standing::Failed,
            Some("killed" | "stopped") => Standing::Stopped,
            _ => return,
        };
        let Some(h) = self.held.iter_mut().find(|h| {
            call.is_some_and(|c| h.agent.call == c)
                || task.is_some_and(|t| h.agent.id.as_deref() == Some(t))
        }) else {
            return;
        };
        if h.agent.standing != Standing::Running {
            return;
        }
        self.endings += 1;
        end(h, standing, at, self.endings);
    }
}

fn end(h: &mut Held, standing: Standing, at: Option<u64>, order: u64) {
    h.agent.standing = standing;
    h.agent.ended_ms = at;
    h.ended = order;
}

fn items(v: &Value) -> &[Value] {
    v["message"]["content"]
        .as_array()
        .map(Vec::as_slice)
        .unwrap_or_default()
}

/// The text of `text` between `open` and the `close` after it.
fn between<'a>(text: &'a str, open: &str, close: &str) -> Option<&'a str> {
    let start = text.find(open)? + open.len();
    let len = text[start..].find(close)?;
    Some(text[start..start + len].trim())
}

/// What the subagents of a session's transcript at `path` stand at: the
/// subagents of the lines taken in, each with its latest steps read from
/// its own transcript.
#[derive(Debug, Clone)]
pub struct Subagents {
    /// Where Claude Code keeps the session's subagents' transcripts.
    dir: PathBuf,
    found: Collector,
    /// Each subagent's steps, by its id, with how long its transcript was.
    steps: HashMap<String, (u64, Vec<Step>)>,
}

impl Subagents {
    /// The subagents of the session whose transcript is at `transcript`.
    pub fn new(transcript: &Path) -> Self {
        Subagents {
            dir: transcript.with_extension("").join("subagents"),
            found: Collector::default(),
            steps: HashMap::new(),
        }
    }

    pub fn feed(&mut self, v: &Value) {
        self.found.feed(v);
    }

    /// Read again the transcript of each subagent listed that grew; whether
    /// any did.
    pub fn look(&mut self) -> bool {
        let mut changed = false;
        let mut kept = HashMap::new();
        for id in self.found.agents().into_iter().filter_map(|a| a.id) {
            let path = self.dir.join(format!("agent-{id}.jsonl"));
            let len = std::fs::metadata(&path).map_or(0, |m| m.len());
            let entry = match self.steps.remove(&id) {
                Some(seen) if seen.0 == len => seen,
                _ => {
                    changed = true;
                    (len, steps_at(&path))
                }
            };
            kept.insert(id, entry);
        }
        changed |= !self.steps.is_empty();
        self.steps = kept;
        changed
    }

    pub fn agents(&self) -> Vec<Agent> {
        let mut agents = self.found.agents();
        for a in &mut agents {
            if let Some((_, steps)) = a.id.as_ref().and_then(|id| self.steps.get(id)) {
                a.steps = steps.clone();
            }
        }
        agents
    }
}

/// Whether any subagent of a session still runs, read from its transcript as
/// it grows: lighter than [`Subagents`], which also reads each subagent's own
/// transcript, so that it can follow every session, not only those shown.
#[derive(Debug)]
pub struct Behind {
    path: PathBuf,
    tail: crate::tail::Tail,
    found: Collector,
}

impl Behind {
    /// Following the transcript at `transcript`.
    pub fn new(transcript: &Path) -> Self {
        Behind {
            path: transcript.to_path_buf(),
            tail: crate::tail::Tail::default(),
            found: Collector::default(),
        }
    }

    /// Take in what was written since the last read; whether a subagent
    /// still runs. One that shrank or was replaced is read from its start.
    pub fn running(&mut self) -> bool {
        match self.tail.read(&self.path) {
            crate::tail::Tailed::Lines {
                restarted, values, ..
            } => {
                if restarted {
                    self.found = Collector::default();
                }
                for v in &values {
                    self.found.feed(v);
                }
            }
            crate::tail::Tailed::Missing => self.found = Collector::default(),
            crate::tail::Tailed::Unreadable => {}
        }
        self.found
            .held
            .iter()
            .any(|h| h.agent.standing == Standing::Running)
    }
}

/// The latest steps in the end of the subagent transcript at `path`.
fn steps_at(path: &Path) -> Vec<Step> {
    use std::io::{Read, Seek, SeekFrom};
    let Ok(mut file) = std::fs::File::open(path) else {
        return Vec::new();
    };
    let len = file.metadata().map_or(0, |m| m.len());
    let from = len.saturating_sub(TAIL);
    let mut bytes = Vec::new();
    if file.seek(SeekFrom::Start(from)).is_err() || file.read_to_end(&mut bytes).is_err() {
        return Vec::new();
    }
    let text = String::from_utf8_lossy(&bytes);
    let mut lines: Vec<&str> = text.lines().collect();
    // Reading from inside the file starts within a line.
    if from > 0 && !lines.is_empty() {
        lines.remove(0);
    }
    steps(lines)
}

/// The latest [`STEPS`] steps the lines of a subagent's transcript tell.
pub fn steps<'a>(lines: impl IntoIterator<Item = &'a str>) -> Vec<Step> {
    let mut out = Vec::new();
    for line in lines {
        let Ok(v) = serde_json::from_str::<Value>(line) else {
            continue;
        };
        if v["type"].as_str() != Some("assistant") {
            continue;
        }
        for item in items(&v) {
            let step = match item["type"].as_str() {
                Some("text") => item["text"].as_str().map(|t| Step::Said(clip(t.trim()))),
                Some("tool_use") => Some(tool_step(item)),
                _ => None,
            };
            match step {
                Some(Step::Said(t)) if t.is_empty() => {}
                Some(step) => out.push(step),
                None => {}
            }
        }
    }
    let drop = out.len().saturating_sub(STEPS);
    out.drain(..drop);
    out
}

fn tool_step(item: &Value) -> Step {
    let name = item["name"].as_str().unwrap_or_default();
    let input = &item["input"];
    // Its report, which it hands back through a tool of its own.
    if name == "SubagentHandback"
        && let Some(m) = input["message"].as_str()
    {
        return Step::Said(clip(m.trim()));
    }
    const ABOUT: &[&str] = &[
        "description",
        "command",
        "file_path",
        "notebook_path",
        "pattern",
        "url",
        "query",
        "skill",
        "to",
        "prompt",
    ];
    let about = ABOUT
        .iter()
        .find_map(|k| input[*k].as_str().filter(|s| !s.trim().is_empty()))
        .and_then(|s| s.lines().map(str::trim).find(|l| !l.is_empty()))
        .map(clip)
        .unwrap_or_default();
    Step::Tool {
        name: name.to_string(),
        about,
    }
}

/// The first [`STEP_TEXT`] bytes of `text`, cut at a character.
fn clip(text: &str) -> String {
    if text.len() <= STEP_TEXT {
        return text.to_string();
    }
    let mut end = STEP_TEXT;
    while !text.is_char_boundary(end) {
        end -= 1;
    }
    format!("{}…", &text[..end])
}

#[cfg(test)]
mod tests;
