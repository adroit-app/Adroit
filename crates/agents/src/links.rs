//! The artifacts, links and files a session's conversation brought up, read
//! from its transcript in the shape Claude Code writes one: every agent's
//! reader puts its own records to the collector in that shape.

use std::collections::{HashMap, HashSet, VecDeque};
use std::ops::Range;
use std::path::{Component, Path, PathBuf};

use serde_json::Value;

pub use adroit_model::links::*;

mod scan;
use scan::*;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
enum Group {
    Artifact,
    Url,
    File,
}

/// What the lines of a transcript bring up, taken in a line at a time.
#[derive(Debug, Clone)]
pub struct Collector {
    cwd: PathBuf,
    home: PathBuf,
    /// How many times something was brought up, which orders them.
    count: u64,
    found: HashMap<(Group, String), (u64, Link)>,
    /// What each artifact publish called its page, by the tool call's id.
    publishing: HashMap<String, String>,
    /// When each file was last brought up, by its path.
    stamps: HashMap<String, u64>,
    /// When the line being taken in was written.
    now_ms: Option<u64>,
    /// Who each message not yet delivered is to, by the tool call's id.
    sending: HashMap<String, String>,
    /// The tool calls whose result is a file's or a page's contents, by id:
    /// what they hold was not brought up by the conversation.
    reading: HashSet<String>,
    /// The sessions messaged or heard from, by name, with when last.
    peers: HashMap<String, (u64, Peer)>,
    /// What was said to the session, oldest first.
    prompts: Vec<Prompt>,
    /// The latest turns, oldest first, and how many there were in all.
    turns: VecDeque<Turn>,
    turned: u64,
    /// Whether what is said next goes on with the reply said so far, as
    /// nothing was run since.
    replying: bool,
}

impl Collector {
    /// A collector for a session working in `cwd`, `~` standing for `home`.
    pub fn new(cwd: impl Into<PathBuf>, home: impl Into<PathBuf>) -> Self {
        Collector {
            cwd: cwd.into(),
            home: home.into(),
            count: 0,
            found: HashMap::new(),
            stamps: HashMap::new(),
            now_ms: None,
            publishing: HashMap::new(),
            sending: HashMap::new(),
            reading: HashSet::new(),
            peers: HashMap::new(),
            prompts: Vec::new(),
            turns: VecDeque::new(),
            turned: 0,
            replying: false,
        }
    }

    /// Take in one line of the transcript; `exists` tells whether a path
    /// named in a reply is there.
    pub fn feed(&mut self, line: &str, exists: &dyn Fn(&Path) -> bool) {
        if let Ok(v) = serde_json::from_str::<Value>(line) {
            self.take(&v, exists);
        }
    }

    /// Take in a record another agent wrote, put in the shape of the line
    /// Claude Code writes for `role`: `content` said at `timestamp`.
    pub(crate) fn said(
        &mut self,
        role: &str,
        timestamp: &Value,
        content: Value,
        exists: &dyn Fn(&Path) -> bool,
    ) {
        self.take(
            &serde_json::json!({"type": role, "timestamp": timestamp, "message": {"content": content}}),
            exists,
        );
    }

    pub(crate) fn take(&mut self, v: &Value, exists: &dyn Fn(&Path) -> bool) {
        let assistant = match v["type"].as_str() {
            Some("assistant") => true,
            Some("user") => false,
            _ => return,
        };
        self.now_ms = v["timestamp"].as_str().and_then(crate::time::epoch_ms);
        let origin = &v["origin"];
        if origin["kind"].as_str() == Some("peer")
            && let Some(name) = origin["name"].as_str()
        {
            self.peer(name, false);
        }
        self.turn(assistant, v);
        // Text Claude Code put in the user's turn, as a skill's or a
        // command's, is not what was said.
        if v["isMeta"].as_bool() == Some(true) {
            return;
        }
        if !assistant && origin["kind"].as_str() != Some("peer") {
            self.prompted(&v["message"]["content"]);
        }
        match &v["message"]["content"] {
            Value::String(s) => self.text(s, assistant, exists),
            Value::Array(items) => {
                for item in items {
                    match item["type"].as_str() {
                        Some("text") => {
                            if let Some(s) = item["text"].as_str() {
                                self.text(s, assistant, exists);
                            }
                        }
                        Some("tool_use") => self.tool_use(item),
                        Some("tool_result") => {
                            self.delivered(item, &v["toolUseResult"]);
                            self.tool_result(item);
                        }
                        _ => {}
                    }
                }
            }
            _ => {}
        }
    }

    /// Forget every line taken in, as a transcript read again from its start
    /// needs.
    pub(crate) fn clear(&mut self) {
        *self = Collector::new(
            std::mem::take(&mut self.cwd),
            std::mem::take(&mut self.home),
        );
    }

    pub fn links(&self) -> Links {
        let mut sorted: Vec<&(u64, Link)> = self.found.values().collect();
        sorted.sort_by_key(|(at, _)| std::cmp::Reverse(*at));
        let mut out = Links::default();
        for (_, link) in sorted {
            let group = match link.kind {
                Kind::Artifact => &mut out.artifacts,
                Kind::Url => &mut out.urls,
                Kind::File(touch) if is_image(&link.target) => {
                    if out.images.len() < KEPT {
                        out.images.push(ImageFile {
                            path: link.target.clone(),
                            touch,
                            at_ms: self.stamps.get(&link.target).copied(),
                        });
                    }
                    continue;
                }
                Kind::File(_) => &mut out.files,
            };
            if group.len() < KEPT {
                group.push(link.clone());
            }
        }
        let mut peers: Vec<&(u64, Peer)> = self.peers.values().collect();
        peers.sort_by_key(|(at, _)| std::cmp::Reverse(*at));
        out.peers = peers.into_iter().map(|(_, p)| p.clone()).collect();
        out.prompts = self.prompts.iter().rev().take(KEPT).cloned().collect();
        out
    }

    /// Take in that a tool was run, where nothing else of it is brought up.
    pub(crate) fn ran(&mut self) {
        self.replying = false;
    }

    /// The latest turns, oldest first.
    pub fn turns(&self) -> Vec<Turn> {
        self.turns.iter().cloned().collect()
    }

    /// Take in what the line `v` adds to the turns. A subagent's line is no
    /// part of them.
    fn turn(&mut self, assistant: bool, v: &Value) {
        if v["isSidechain"].as_bool() == Some(true) {
            return;
        }
        let content = &v["message"]["content"];
        if !assistant {
            let channel = v["origin"]["kind"].as_str() == Some("channel");
            let began = match channel || v["isMeta"].as_bool() != Some(true) {
                true => prompt_of(content).filter(|said| !put_there_by_codex(said)),
                false => None,
            };
            if let Some(prompt) = began {
                let prompt = match channel {
                    true => without_channel(&prompt).to_string(),
                    false => prompt,
                };
                self.turned += 1;
                self.turns.push_back(Turn {
                    turn: self.turned,
                    prompt,
                    reply: None,
                    began_ms: self.now_ms,
                    last_ms: self.now_ms,
                });
                if self.turns.len() > TURNS {
                    self.turns.pop_front();
                }
            }
            self.replying = false;
        }
        let Some(turn) = self.turns.back_mut() else {
            return;
        };
        turn.last_ms = self.now_ms.or(turn.last_ms);
        if !assistant {
            return;
        }
        for item in content.as_array().into_iter().flatten() {
            match item["type"].as_str() {
                Some("text") => {
                    let said = item["text"].as_str().unwrap_or_default().trim();
                    if said.is_empty() {
                        continue;
                    }
                    turn.reply = Some(match turn.reply.take().filter(|_| self.replying) {
                        Some(before) => format!("{before}\n\n{said}"),
                        None => said.to_string(),
                    });
                    self.replying = true;
                }
                Some("tool_use") => self.replying = false,
                _ => {}
            }
        }
    }

    /// What the user said in a turn of `content`, kept as a prompt.
    fn prompted(&mut self, content: &Value) {
        let Some(text) = prompt_of(content) else {
            return;
        };
        self.prompts.push(Prompt {
            turn: self.prompts.len() as u64 + 1,
            text,
            at_ms: self.now_ms,
        });
    }

    fn text(&mut self, text: &str, assistant: bool, exists: &dyn Fn(&Path) -> bool) {
        if !assistant {
            self.addresses(&without_reminders(text), None);
            return;
        }
        let spans = self.addresses(text, None);
        let mut rest = text.to_string();
        for span in spans.into_iter().rev() {
            rest.replace_range(span, " ");
        }
        self.mentioned(&rest, exists);
    }

    fn tool_use(&mut self, item: &Value) {
        let input = &item["input"];
        let name = item["name"].as_str().unwrap_or_default();
        let touch = match name {
            "Write" | "Edit" | "MultiEdit" | "NotebookEdit" => Some(Touch::Edited),
            "Read" => Some(Touch::Read),
            _ => None,
        };
        let path = input["file_path"]
            .as_str()
            .or_else(|| input["notebook_path"].as_str());
        if let (Some(touch), Some(path)) = (touch, path) {
            let target = self.resolve(path).to_string_lossy().into_owned();
            self.note(Link {
                kind: Kind::File(touch),
                target,
                title: None,
                line: None,
            });
        }
        let title = (name == "Artifact")
            .then(|| {
                input["title"].as_str().map(str::to_string).or_else(|| {
                    let stem = Path::new(path?).file_stem()?;
                    Some(stem.to_string_lossy().into_owned())
                })
            })
            .flatten();
        if let (Some(title), Some(id)) = (&title, item["id"].as_str()) {
            self.publishing.insert(id.to_string(), title.clone());
        }
        if name == "SendMessage"
            && let (Some(to), Some(id)) = (input["to"].as_str(), item["id"].as_str())
        {
            self.sending.insert(id.to_string(), to.to_string());
        }
        // An address inside a file the session writes is the file's, not the
        // conversation's; so is one in what it reads or searches for. A page
        // fetched comes back summarised for the conversation, so its stay.
        if matches!(name, "Read" | "Grep" | "Glob")
            && let Some(id) = item["id"].as_str()
        {
            self.reading.insert(id.to_string());
        }
        if touch.is_some() {
            return;
        }
        let mut texts = Vec::new();
        strings(input, &mut texts);
        for text in texts {
            self.addresses(head(text), title.as_deref());
        }
    }

    fn tool_result(&mut self, item: &Value) {
        if item["tool_use_id"]
            .as_str()
            .is_some_and(|id| self.reading.remove(id))
        {
            return;
        }
        let text = match &item["content"] {
            Value::String(s) => s.clone(),
            Value::Array(parts) => parts
                .iter()
                .filter_map(|p| p["text"].as_str())
                .collect::<Vec<_>>()
                .join("\n"),
            _ => return,
        };
        let title = item["tool_use_id"]
            .as_str()
            .and_then(|id| self.publishing.get(id))
            .cloned();
        self.addresses(head(&text), title.as_deref());
    }

    /// Note the session a message went to, once it was delivered to one. A
    /// message to an agent inside this session is pinned to that agent.
    fn delivered(&mut self, item: &Value, outcome: &Value) {
        let Some(to) = item["tool_use_id"]
            .as_str()
            .and_then(|id| self.sending.remove(id))
        else {
            return;
        };
        if outcome["success"].as_bool() == Some(true) && outcome["pin"].is_null() {
            self.peer(&to, true);
        }
    }

    fn peer(&mut self, name: &str, sent: bool) {
        let name = without_ref(name);
        if name.is_empty() {
            return;
        }
        self.count += 1;
        let (at, peer) = self.peers.entry(name.to_string()).or_insert_with(|| {
            (
                0,
                Peer {
                    name: name.to_string(),
                    sent: false,
                    received: false,
                },
            )
        });
        *at = self.count;
        if sent {
            peer.sent = true;
        } else {
            peer.received = true;
        }
    }

    /// Note every address in `text`, an artifact titled `title` when nothing
    /// nearer titles it; where each lies in `text`.
    fn addresses(&mut self, text: &str, title: Option<&str>) -> Vec<Range<usize>> {
        let found = urls(text);
        let mut spans = Vec::new();
        for (span, url, own) in found {
            let kind = if artifact_id(&url).is_some() {
                Kind::Artifact
            } else {
                Kind::Url
            };
            let title = match kind {
                Kind::Artifact => own.or_else(|| title.map(str::to_string)),
                _ => own,
            };
            self.note(Link {
                kind,
                target: url,
                title,
                line: None,
            });
            spans.push(span);
        }
        spans
    }

    fn mentioned(&mut self, text: &str, exists: &dyn Fn(&Path) -> bool) {
        let apart = |c: char| {
            c.is_whitespace()
                || matches!(
                    c,
                    '`' | '"'
                        | '\''
                        | '('
                        | ')'
                        | '['
                        | ']'
                        | '<'
                        | '>'
                        | ','
                        | ';'
                        | '|'
                        | '*'
                        | '{'
                        | '}'
                )
        };
        for token in text.split(apart) {
            let token = token.trim_end_matches(['.', ':', '!', '?']);
            let (path, line) = split_line(token);
            if !looks_like_path(path) {
                continue;
            }
            let resolved = self.resolve(path);
            if exists(&resolved) {
                self.note(Link {
                    kind: Kind::File(Touch::Mentioned),
                    target: resolved.to_string_lossy().into_owned(),
                    title: None,
                    line,
                });
            }
        }
    }

    fn resolve(&self, path: &str) -> PathBuf {
        let joined = match path.strip_prefix("~/") {
            Some(rest) => self.home.join(rest),
            None => self.cwd.join(path),
        };
        let mut out = PathBuf::new();
        for c in joined.components() {
            match c {
                Component::CurDir => {}
                Component::ParentDir => {
                    out.pop();
                }
                other => out.push(other),
            }
        }
        out
    }

    fn note(&mut self, link: Link) {
        self.count += 1;
        if let (Kind::File(_), Some(at)) = (link.kind, self.now_ms) {
            self.stamps.insert(link.target.clone(), at);
        }
        let (group, key) = match link.kind {
            Kind::Artifact => (
                Group::Artifact,
                artifact_id(&link.target).unwrap_or_default().to_string(),
            ),
            Kind::Url => (Group::Url, link.target.clone()),
            Kind::File(_) => (Group::File, link.target.clone()),
        };
        match self.found.get_mut(&(group, key.clone())) {
            Some((at, old)) => {
                *at = self.count;
                if let (Kind::File(was), Kind::File(now)) = (old.kind, link.kind) {
                    old.kind = Kind::File(was.max(now));
                }
                old.target = link.target;
                old.title = link.title.or(old.title.take());
                old.line = link.line.or(old.line);
            }
            None => {
                self.found.insert((group, key), (self.count, link));
            }
        }
    }
}

/// The call `id` of another agent's file tool `name` on `path`, as the
/// collector reads Claude Code's own; none for a tool that is not one of
/// them, or a call that names no path.
pub(crate) fn file_call(id: &str, name: &str, path: Option<&str>) -> Option<Value> {
    let name = match name {
        "read" => "Read",
        "edit" => "Edit",
        "write" => "Write",
        _ => return None,
    };
    Some(
        serde_json::json!({"type": "tool_use", "id": id, "name": name, "input": {"file_path": path?}}),
    )
}

#[cfg(test)]
pub(crate) mod tests;
