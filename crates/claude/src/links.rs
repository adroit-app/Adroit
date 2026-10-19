//! The artifacts, links and files a session's conversation brought up, read
//! from the transcript Claude Code writes of it.

use std::collections::{HashMap, HashSet};
use std::ops::Range;
use std::path::{Component, Path, PathBuf};

use serde_json::Value;

use crate::subagents::{Agent, Subagents};

/// How a session came across a file.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Touch {
    /// Named in a reply.
    Mentioned,
    /// Read with Claude Code's `Read`.
    Read,
    /// Written or edited.
    Edited,
}

/// What a link points at.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Kind {
    /// A claude.ai artifact.
    Artifact,
    /// Any other web address.
    Url,
    File(Touch),
}

/// Something a session brought up.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Link {
    pub kind: Kind,
    /// The address, or the file's absolute path.
    pub target: String,
    /// What the conversation called it.
    pub title: Option<String>,
    /// The line of a file it was named at.
    pub line: Option<u32>,
}

/// Another session a session exchanged messages with through Claude Code.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Peer {
    /// The name Claude Code addresses it by.
    pub name: String,
    /// Whether this session messaged it.
    pub sent: bool,
    /// Whether it messaged this session.
    pub received: bool,
}

/// An image file a session came across.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImageFile {
    /// The file's absolute path.
    pub path: String,
    pub touch: Touch,
    /// When the session last came across it, in milliseconds since the Unix
    /// epoch, as the transcript tells.
    pub at_ms: Option<u64>,
}

/// Whether `path` names an image, told by its extension.
pub fn is_image(path: &str) -> bool {
    const IMAGES: &[&str] = &[
        "png", "jpg", "jpeg", "gif", "webp", "bmp", "svg", "heic", "heif", "tif", "tiff", "ico",
        "avif",
    ];
    let p = Path::new(path);
    !path.ends_with('/')
        && p.file_stem().is_some()
        && p.extension()
            .and_then(|e| e.to_str())
            .is_some_and(|e| IMAGES.iter().any(|i| e.eq_ignore_ascii_case(i)))
}

/// What a session brought up, each group newest first, and the subagents
/// it runs.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Links {
    pub artifacts: Vec<Link>,
    pub urls: Vec<Link>,
    pub files: Vec<Link>,
    /// The image files, apart from the other files.
    pub images: Vec<ImageFile>,
    pub peers: Vec<Peer>,
    pub agents: Vec<Agent>,
}

impl Links {
    pub fn is_empty(&self) -> bool {
        self.artifacts.is_empty()
            && self.urls.is_empty()
            && self.files.is_empty()
            && self.images.is_empty()
            && self.peers.is_empty()
            && self.agents.is_empty()
    }

    /// Every link, artifacts first, then the other addresses, then files.
    pub fn rows(&self) -> impl Iterator<Item = &Link> {
        self.artifacts.iter().chain(&self.urls).chain(&self.files)
    }
}

/// The most links each group keeps: the newest. The window shows fewer, as
/// its settings say.
pub const KEPT: usize = 1000;

/// The most of a tool's result or input read for addresses: past it lies
/// output too long to be worth reading for them.
const SCANNED: usize = 64 * 1024;

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
        }
    }

    /// Take in one line of the transcript; `exists` tells whether a path
    /// named in a reply is there.
    pub fn feed(&mut self, line: &str, exists: &dyn Fn(&Path) -> bool) {
        if let Ok(v) = serde_json::from_str::<Value>(line) {
            self.take(&v, exists);
        }
    }

    fn take(&mut self, v: &Value, exists: &dyn Fn(&Path) -> bool) {
        let assistant = match v["type"].as_str() {
            Some("assistant") => true,
            Some("user") => false,
            _ => return,
        };
        self.now_ms = v["timestamp"].as_str().and_then(crate::subagents::epoch_ms);
        let origin = &v["origin"];
        if origin["kind"].as_str() == Some("peer")
            && let Some(name) = origin["name"].as_str()
        {
            self.peer(name, false);
        }
        // Text Claude Code put in the user's turn, as a skill's or a
        // command's, is not what was said.
        if v["isMeta"].as_bool() == Some(true) {
            return;
        }
        match &v["message"]["content"] {
            Value::String(s) => self.said(s, assistant, exists),
            Value::Array(items) => {
                for item in items {
                    match item["type"].as_str() {
                        Some("text") => {
                            if let Some(s) = item["text"].as_str() {
                                self.said(s, assistant, exists);
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
        out
    }

    fn said(&mut self, text: &str, assistant: bool, exists: &dyn Fn(&Path) -> bool) {
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

/// A transcript read as it grows: each read takes in only what was written
/// since the last.
#[derive(Debug, Clone)]
pub struct Transcript {
    path: PathBuf,
    read_to: u64,
    /// The file read, by its inode, to tell one put in its place.
    file: Option<u64>,
    /// The start of a line not written to its end yet.
    rest: Vec<u8>,
    found: Collector,
    agents: Subagents,
}

impl Transcript {
    /// The transcript at `path` of a session working in `cwd`.
    pub fn new(
        path: impl Into<PathBuf>,
        cwd: impl Into<PathBuf>,
        home: impl Into<PathBuf>,
    ) -> Self {
        let path = path.into();
        Transcript {
            agents: Subagents::new(&path),
            path,
            read_to: 0,
            file: None,
            rest: Vec::new(),
            found: Collector::new(cwd, home),
        }
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Take in what was written since the last read; whether what it brings
    /// up may have changed. One that shrank, was replaced or is gone is read
    /// from its start. The transcripts of its subagents are read too.
    pub fn read(&mut self, exists: &dyn Fn(&Path) -> bool) -> bool {
        let changed = self.read_own(exists);
        self.agents.look() || changed
    }

    fn read_own(&mut self, exists: &dyn Fn(&Path) -> bool) -> bool {
        use std::io::{Read, Seek, SeekFrom};
        use std::os::unix::fs::MetadataExt;
        let opened = std::fs::File::open(&self.path).and_then(|f| Ok((f.metadata()?, f)));
        let Ok((meta, file)) = opened else {
            let had = self.file.is_some();
            self.restart();
            return had;
        };
        let mut changed = false;
        if meta.len() < self.read_to || self.file.is_some_and(|ino| ino != meta.ino()) {
            self.restart();
            changed = true;
        }
        self.file = Some(meta.ino());
        if meta.len() == self.read_to {
            return changed;
        }
        let mut file = file;
        let mut new = Vec::new();
        let read = file
            .seek(SeekFrom::Start(self.read_to))
            .and_then(|_| file.take(meta.len() - self.read_to).read_to_end(&mut new));
        if read.is_err() {
            return changed;
        }
        self.read_to += new.len() as u64;
        self.rest.extend(new);
        let Some(end) = self.rest.iter().rposition(|b| *b == b'\n') else {
            return changed;
        };
        let done: Vec<u8> = self.rest.drain(..=end).collect();
        for line in String::from_utf8_lossy(&done).lines() {
            if let Ok(v) = serde_json::from_str::<Value>(line) {
                self.found.take(&v, exists);
                self.agents.feed(&v);
            }
        }
        true
    }

    pub fn links(&self) -> Links {
        Links {
            agents: self.agents.agents(),
            ..self.found.links()
        }
    }

    fn restart(&mut self) {
        self.read_to = 0;
        self.file = None;
        self.rest.clear();
        self.agents = Subagents::new(&self.path);
        self.found = Collector::new(
            std::mem::take(&mut self.found.cwd),
            std::mem::take(&mut self.found.home),
        );
    }
}

/// Every string in `v`, at any depth.
fn strings<'a>(v: &'a Value, out: &mut Vec<&'a str>) {
    match v {
        Value::String(s) => out.push(s),
        Value::Array(items) => items.iter().for_each(|i| strings(i, out)),
        Value::Object(map) => map.values().for_each(|i| strings(i, out)),
        _ => {}
    }
}

/// A session's name without the ` [ref]` Claude Code adds to tell apart
/// sessions sharing it.
fn without_ref(name: &str) -> &str {
    let name = name.trim();
    match name.rsplit_once(" [") {
        Some((bare, r))
            if r.strip_suffix(']')
                .is_some_and(|r| !r.is_empty() && r.bytes().all(|b| b.is_ascii_hexdigit())) =>
        {
            bare.trim_end()
        }
        _ => name,
    }
}

/// The first [`SCANNED`] bytes of `text`, cut at a character.
fn head(text: &str) -> &str {
    if text.len() <= SCANNED {
        return text;
    }
    let mut end = SCANNED;
    while !text.is_char_boundary(end) {
        end -= 1;
    }
    &text[..end]
}

fn without_reminders(text: &str) -> String {
    const OPEN: &str = "<system-reminder>";
    const CLOSE: &str = "</system-reminder>";
    let mut out = String::new();
    let mut rest = text;
    while let Some(start) = rest.find(OPEN) {
        out.push_str(&rest[..start]);
        rest = match rest[start..].find(CLOSE) {
            Some(end) => &rest[start + end + CLOSE.len()..],
            None => "",
        };
    }
    out.push_str(rest);
    out
}

/// The web addresses in `text`: where each lies, the address, and the text
/// of the Markdown link it is the target of.
fn urls(text: &str) -> Vec<(Range<usize>, String, Option<String>)> {
    let mut out = Vec::new();
    let mut from = 0;
    while let Some(i) = text[from..].find("http") {
        let start = from + i;
        let rest = &text[start..];
        let scheme = if rest.starts_with("https://") {
            8
        } else if rest.starts_with("http://") {
            7
        } else {
            from = start + 4;
            continue;
        };
        from = start + scheme;
        if text[..start]
            .chars()
            .next_back()
            .is_some_and(char::is_alphanumeric)
        {
            continue;
        }
        let len = rest
            .find(|c: char| {
                c.is_whitespace()
                    || matches!(c, '<' | '>' | '"' | '\'' | '`' | '\\' | '|' | '{' | '}')
            })
            .unwrap_or(rest.len());
        // A Markdown link whose text is its own address.
        let raw = &rest[..len];
        let raw = raw.find("](").map_or(raw, |i| &raw[..i]);
        let url = trim_url(raw);
        if host(url).is_empty() {
            continue;
        }
        from = start + url.len();
        out.push((
            start..start + url.len(),
            url.to_string(),
            markdown_title(&text[..start], url),
        ));
    }
    out
}

/// `url` without what closes the sentence or the brackets around it.
fn trim_url(mut url: &str) -> &str {
    loop {
        let count = |c: char| url.matches(c).count();
        let Some(last) = url.chars().next_back() else {
            return url;
        };
        let drop = match last {
            '.' | ',' | ';' | ':' | '!' | '?' | '*' | '~' => true,
            ')' => count('(') < count(')'),
            ']' => count('[') < count(']'),
            _ => false,
        };
        if !drop {
            return url;
        }
        url = &url[..url.len() - last.len_utf8()];
    }
}

fn host(url: &str) -> &str {
    let rest = url.split_once("://").map_or("", |(_, r)| r);
    rest.split(['/', '?', '#']).next().unwrap_or_default()
}

fn markdown_title(before: &str, url: &str) -> Option<String> {
    let inner = before.strip_suffix("](")?;
    let label = &inner[inner.rfind('[')? + 1..];
    if label.contains(['\n', ']']) {
        return None;
    }
    let label = label.trim().trim_matches(['`', '*']).trim();
    (!label.is_empty() && label != url).then(|| label.to_string())
}

/// The id of a claude.ai artifact `url` names.
pub fn artifact_id(url: &str) -> Option<&str> {
    let rest = url
        .strip_prefix("https://claude.ai/artifact/")
        .or_else(|| url.strip_prefix("https://claude.ai/code/artifact/"))?;
    let id = rest.split(['/', '?', '#']).next()?;
    (!id.is_empty()).then_some(id)
}

/// `token` without a `:line` or `:line:column` it ends with, and the line.
fn split_line(token: &str) -> (&str, Option<u32>) {
    let number = |s: &str| !s.is_empty() && s.bytes().all(|b| b.is_ascii_digit());
    let mut path = token;
    let mut line = None;
    for _ in 0..2 {
        match path.rsplit_once(':') {
            Some((p, n)) if number(n) => {
                line = n.parse().ok();
                path = p;
            }
            _ => break,
        }
    }
    (path, line)
}

fn looks_like_path(token: &str) -> bool {
    if token.is_empty() || token.len() > 1024 || token.contains("://") {
        return false;
    }
    let p = Path::new(token);
    token.contains('/')
        || p.extension().is_some_and(|e| {
            e.to_str()
                .is_some_and(|e| e.bytes().all(|b| b.is_ascii_alphanumeric()))
        }) && p.file_stem().is_some_and(|s| !s.is_empty())
}

/// What `lines` of a transcript bring up, for a session working in `cwd`.
pub fn links<'a>(
    lines: impl IntoIterator<Item = &'a str>,
    cwd: &Path,
    home: &Path,
    exists: impl Fn(&Path) -> bool,
) -> Links {
    let mut c = Collector::new(cwd, home);
    for line in lines {
        c.feed(line, &exists);
    }
    c.links()
}

#[cfg(test)]
mod tests {
    use super::*;

    use serde_json::json;

    fn assistant(content: serde_json::Value) -> String {
        json!({"type": "assistant", "message": {"role": "assistant", "content": content}})
            .to_string()
    }

    fn user(content: serde_json::Value) -> String {
        json!({"type": "user", "message": {"role": "user", "content": content}}).to_string()
    }

    fn said(text: &str) -> String {
        assistant(json!([{"type": "text", "text": text}]))
    }

    fn tool(id: &str, name: &str, input: serde_json::Value) -> String {
        assistant(json!([{"type": "tool_use", "id": id, "name": name, "input": input}]))
    }

    fn result(id: &str, content: serde_json::Value) -> String {
        user(json!([{"type": "tool_result", "tool_use_id": id, "content": content}]))
    }

    fn read(lines: &[String], on_disk: &[&str]) -> Links {
        links(
            lines.iter().map(String::as_str),
            Path::new("/w"),
            Path::new("/home/me"),
            |p| on_disk.iter().any(|d| Path::new(d) == p),
        )
    }

    fn targets(links: &[Link]) -> Vec<&str> {
        links.iter().map(|l| l.target.as_str()).collect()
    }

    fn file(path: &str, touch: Touch, line: Option<u32>) -> Link {
        Link {
            kind: Kind::File(touch),
            target: path.into(),
            title: None,
            line,
        }
    }

    #[test]
    fn an_address_inside_a_file_written_read_or_searched_is_not_brought_up() {
        let got = read(
            &[
                tool(
                    "t1",
                    "Edit",
                    json!({"file_path": "/w/a.rs", "old_string": "x", "new_string": "see https://claude.ai/artifact/fake"}),
                ),
                tool(
                    "t2",
                    "Write",
                    json!({"file_path": "/w/b.md", "content": "https://written.dev"}),
                ),
                tool("t3", "Read", json!({"file_path": "/w/c.md"})),
                result("t3", json!("1\thttps://read.dev")),
                tool("t4", "Grep", json!({"pattern": "http"})),
                result("t4", json!("c.md:1:https://grepped.dev")),
                // What a command printed, a page summarised for the conversation and
                // what the session said are brought up.
                tool(
                    "t5",
                    "WebFetch",
                    json!({"url": "https://fetched.dev/page", "prompt": "links"}),
                ),
                result("t5", json!("The page links https://inside.dev")),
                tool("t6", "Bash", json!({"command": "gh pr create"})),
                result("t6", json!("https://github.com/o/r/pull/1")),
                said("Published https://claude.ai/artifact/real"),
            ],
            &[],
        );
        assert_eq!(targets(&got.artifacts), ["https://claude.ai/artifact/real"]);
        assert_eq!(
            targets(&got.urls),
            [
                "https://github.com/o/r/pull/1",
                "https://inside.dev",
                "https://fetched.dev/page"
            ]
        );
    }

    #[test]
    fn a_file_written_or_edited_is_listed_as_edited() {
        let got = read(
            &[
                tool(
                    "t1",
                    "Write",
                    json!({"file_path": "/w/src/a.rs", "content": ""}),
                ),
                tool("t2", "Edit", json!({"file_path": "src/b.rs"})),
                tool("t3", "NotebookEdit", json!({"notebook_path": "/w/n.ipynb"})),
            ],
            &[],
        );
        assert_eq!(
            got.files,
            [
                file("/w/n.ipynb", Touch::Edited, None),
                file("/w/src/b.rs", Touch::Edited, None),
                file("/w/src/a.rs", Touch::Edited, None),
            ]
        );
    }

    #[test]
    fn a_file_read_and_later_edited_is_listed_once_as_edited() {
        let got = read(
            &[
                tool("t1", "Edit", json!({"file_path": "/w/a.rs"})),
                tool("t2", "Read", json!({"file_path": "/w/a.rs"})),
                tool("t3", "Read", json!({"file_path": "/w/b.rs"})),
            ],
            &[],
        );
        assert_eq!(
            got.files,
            [
                file("/w/b.rs", Touch::Read, None),
                file("/w/a.rs", Touch::Edited, None),
            ]
        );
    }

    #[test]
    fn a_path_named_in_a_reply_is_listed_only_when_it_is_there() {
        let got = read(
            &[said(
                "See `crates/x/y.rs:12:3`, then nope/z.rs and ~/notes/a.md. Also /etc/hosts.",
            )],
            &["/w/crates/x/y.rs", "/home/me/notes/a.md", "/etc/hosts"],
        );
        assert_eq!(
            got.files,
            [
                file("/etc/hosts", Touch::Mentioned, None),
                file("/home/me/notes/a.md", Touch::Mentioned, None),
                file("/w/crates/x/y.rs", Touch::Mentioned, Some(12)),
            ]
        );
    }

    #[test]
    fn a_relative_path_is_taken_from_the_session_s_directory_dots_resolved() {
        let got = read(&[said("Look at ./src/../lib.rs.")], &["/w/lib.rs"]);
        assert_eq!(targets(&got.files), ["/w/lib.rs"]);
    }

    #[test]
    fn a_path_only_the_user_or_a_tool_names_is_not_taken_as_mentioned() {
        let got = read(
            &[user(json!("open /w/a.rs")), result("t1", json!("/w/a.rs"))],
            &["/w/a.rs"],
        );
        assert!(got.files.is_empty());
    }

    #[test]
    fn an_address_sheds_the_punctuation_and_brackets_around_it() {
        let got = read(
            &[said(
                "(see https://a.dev/x). <https://b.dev/y> `https://c.dev/z`, \
                 https://d.dev/w/Rust_(language) and \"https://e.dev/q?a=1\"!",
            )],
            &[],
        );
        assert_eq!(
            targets(&got.urls),
            [
                "https://e.dev/q?a=1",
                "https://d.dev/w/Rust_(language)",
                "https://c.dev/z",
                "https://b.dev/y",
                "https://a.dev/x",
            ]
        );
    }

    #[test]
    fn a_markdown_link_is_titled_by_its_text() {
        let got = read(
            &[said(
                "Read [the guide](https://a.dev/guide) or [https://b.dev](https://b.dev).",
            )],
            &[],
        );
        let titles: Vec<_> = got.urls.iter().map(|l| l.title.as_deref()).collect();
        assert_eq!(targets(&got.urls), ["https://b.dev", "https://a.dev/guide"]);
        assert_eq!(titles, [None, Some("the guide")]);
    }

    #[test]
    fn addresses_come_from_the_user_tool_inputs_and_tool_results_too() {
        let got = read(
            &[
                user(json!("look at https://u.dev")),
                tool(
                    "t1",
                    "WebFetch",
                    json!({"url": "https://i.dev/a", "prompt": "x"}),
                ),
                result(
                    "t1",
                    json!([{"type": "text", "text": "moved to https://r.dev/b"}]),
                ),
            ],
            &[],
        );
        assert_eq!(
            targets(&got.urls),
            ["https://r.dev/b", "https://i.dev/a", "https://u.dev"]
        );
    }

    #[test]
    fn an_artifact_is_its_own_group_titled_by_the_publish_that_made_it() {
        let got = read(
            &[
                tool(
                    "t1",
                    "Artifact",
                    json!({"file_path": "/s/report.html", "title": "Weekly report"}),
                ),
                result(
                    "t1",
                    json!("Published https://claude.ai/code/artifact/abc-123"),
                ),
                tool("t2", "Artifact", json!({"file_path": "/s/board.html"})),
                result("t2", json!("Published https://claude.ai/code/artifact/def")),
                said("Also https://claude.ai/artifact/xyz?v=2"),
            ],
            &[],
        );
        assert!(got.urls.is_empty());
        let titles: Vec<_> = got.artifacts.iter().map(|l| l.title.as_deref()).collect();
        assert_eq!(
            targets(&got.artifacts),
            [
                "https://claude.ai/artifact/xyz?v=2",
                "https://claude.ai/code/artifact/def",
                "https://claude.ai/code/artifact/abc-123",
            ]
        );
        assert_eq!(titles, [None, Some("board"), Some("Weekly report")]);
    }

    #[test]
    fn something_brought_up_again_moves_to_the_front_once() {
        let got = read(
            &[
                said("https://a.dev"),
                said("https://b.dev"),
                said("https://a.dev again"),
            ],
            &[],
        );
        assert_eq!(targets(&got.urls), ["https://a.dev", "https://b.dev"]);
    }

    #[test]
    fn malformed_lines_other_records_injected_text_and_thinking_are_passed_over() {
        let got = read(
            &[
                "not json".into(),
                json!({"type": "summary", "summary": "https://s.dev"}).to_string(),
                json!({"type": "user", "isMeta": true, "message": {"content": "https://m.dev"}})
                    .to_string(),
                user(json!(
                    "<system-reminder>see https://r.dev</system-reminder>and https://kept.dev"
                )),
                assistant(json!([{"type": "thinking", "thinking": "https://t.dev"}])),
                assistant(json!("no array")),
            ],
            &[],
        );
        assert_eq!(targets(&got.urls), ["https://kept.dev"]);
    }

    #[test]
    fn only_the_head_of_a_large_tool_result_is_read() {
        let big = format!("https://head.dev {} https://tail.dev", "x".repeat(SCANNED));
        let got = read(&[result("t1", json!(big))], &[]);
        assert_eq!(targets(&got.urls), ["https://head.dev"]);
    }

    #[test]
    fn each_group_keeps_its_newest() {
        let lines: Vec<String> = (0..KEPT + 5)
            .map(|i| said(&format!("https://a.dev/{i}")))
            .collect();
        let got = read(&lines, &[]);
        assert_eq!(got.urls.len(), KEPT);
        assert_eq!(got.urls[0].target, format!("https://a.dev/{}", KEPT + 4));
    }

    #[test]
    fn the_rows_run_artifacts_then_addresses_then_files() {
        let got = read(
            &[
                tool("t1", "Read", json!({"file_path": "/w/a.rs"})),
                said("https://a.dev https://claude.ai/artifact/x"),
            ],
            &[],
        );
        let kinds: Vec<_> = got.rows().map(|l| l.kind).collect();
        assert_eq!(kinds, [Kind::Artifact, Kind::Url, Kind::File(Touch::Read)]);
    }
    fn scratch(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("adroit-links-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir.join("talk.jsonl")
    }

    fn append(path: &Path, text: &str) {
        use std::io::Write;
        let mut f = std::fs::File::options()
            .create(true)
            .append(true)
            .open(path)
            .unwrap();
        f.write_all(text.as_bytes()).unwrap();
    }

    const NOWHERE: &dyn Fn(&Path) -> bool = &|_| false;

    #[test]
    fn a_transcript_is_read_on_from_where_it_was_left_a_partial_line_awaiting_its_end() {
        let path = scratch("appended");
        append(&path, &format!("{}\n", said("https://a.dev")));
        let mut t = Transcript::new(&path, "/w", "/home/me");
        assert!(t.read(NOWHERE));
        assert!(!t.read(NOWHERE), "nothing new");
        let line = said("https://b.dev");
        let (first, second) = line.split_at(10);
        append(&path, first);
        assert!(!t.read(NOWHERE), "half a line");
        append(&path, &format!("{second}\n"));
        assert!(t.read(NOWHERE));
        assert_eq!(targets(&t.links().urls), ["https://b.dev", "https://a.dev"]);
    }

    #[test]
    fn a_transcript_that_shrank_or_was_replaced_is_read_again_from_its_start() {
        let path = scratch("replaced");
        append(
            &path,
            &format!("{}\n{}\n", said("https://a.dev"), said("https://b.dev")),
        );
        let mut t = Transcript::new(&path, "/w", "/home/me");
        assert!(t.read(NOWHERE));
        std::fs::write(&path, format!("{}\n", said("https://c.dev"))).unwrap();
        assert!(t.read(NOWHERE));
        assert_eq!(targets(&t.links().urls), ["https://c.dev"]);
        std::fs::remove_file(&path).unwrap();
        assert!(t.read(NOWHERE));
        assert!(t.links().is_empty());
    }

    #[test]
    fn a_transcript_lists_the_subagents_it_runs_with_their_steps() {
        use crate::subagents::{Standing, Step};
        let path = scratch("agents");
        let own = path.with_extension("").join("subagents");
        std::fs::create_dir_all(&own).unwrap();
        append(
            &path,
            &format!(
                "{}\n{}\n",
                tool(
                    "toolu_a",
                    "Agent",
                    json!({"description": "Survey", "subagent_type": "Explore", "prompt": "Look."})
                ),
                json!({
                    "type": "user",
                    "message": {"role": "user", "content": [
                        {"type": "tool_result", "tool_use_id": "toolu_a", "content": "launched"}
                    ]},
                    "toolUseResult": {"isAsync": true, "status": "async_launched", "agentId": "a7cd"},
                }),
            ),
        );
        let mut t = Transcript::new(&path, "/w", "/home/me");
        assert!(t.read(NOWHERE));
        assert_eq!(t.links().agents[0].standing, Standing::Running);
        assert!(t.links().agents[0].steps.is_empty());
        assert!(!t.links().is_empty());
        append(
            &own.join("agent-a7cd.jsonl"),
            &format!("{}\n", said("Reading the code.")),
        );
        assert!(t.read(NOWHERE), "its own transcript grew");
        assert_eq!(
            t.links().agents[0].steps,
            [Step::Said("Reading the code.".into())]
        );
    }

    fn sent(id: &str, to: &str) -> String {
        tool(
            id,
            "SendMessage",
            json!({"to": to, "summary": "s", "message": "m"}),
        )
    }

    fn delivered(id: &str, outcome: serde_json::Value) -> String {
        json!({
            "type": "user",
            "message": {"role": "user", "content": [
                {"type": "tool_result", "tool_use_id": id, "content": "delivered"}
            ]},
            "toolUseResult": outcome,
        })
        .to_string()
    }

    fn arrived(from: &str) -> String {
        json!({
            "type": "user",
            "isMeta": true,
            "message": {"role": "user", "content": "Another Claude session sent a message: …"},
            "origin": {"kind": "peer", "name": from, "body": "b"},
        })
        .to_string()
    }

    fn peer(name: &str, sent: bool, received: bool) -> Peer {
        Peer {
            name: name.into(),
            sent,
            received,
        }
    }

    #[test]
    fn a_session_messaged_and_one_heard_from_are_its_peers_newest_first() {
        let links = read(
            &[
                sent("t1", "api-tests"),
                delivered("t1", json!({"success": true, "msg_id": "m1"})),
                arrived("review-api"),
            ],
            &[],
        );
        assert_eq!(
            links.peers,
            [
                peer("review-api", false, true),
                peer("api-tests", true, false)
            ]
        );
        assert!(!links.is_empty());
    }

    #[test]
    fn a_peer_messaged_both_ways_is_listed_once() {
        let links = read(
            &[
                arrived("api-tests"),
                sent("t1", "api-tests"),
                delivered("t1", json!({"success": true, "msg_id": "m1"})),
            ],
            &[],
        );
        assert_eq!(links.peers, [peer("api-tests", true, true)]);
    }

    #[test]
    fn a_peer_addressed_with_its_ref_goes_by_its_name() {
        let links = read(
            &[
                sent("t1", "api-tests [17b2f1]"),
                delivered("t1", json!({"success": true, "msg_id": "m1"})),
            ],
            &[],
        );
        assert_eq!(links.peers, [peer("api-tests", true, false)]);
    }

    #[test]
    fn a_message_that_failed_or_went_to_an_agent_inside_the_session_makes_no_peer() {
        let links = read(
            &[
                sent("t1", "nobody"),
                delivered("t1", json!({"success": false})),
                sent("t2", "a1b2c3"),
                delivered(
                    "t2",
                    json!({"success": true, "pin": {"id": "a1b2c3", "name": "a1b2c3"}}),
                ),
                sent("t3", "never-answered"),
            ],
            &[],
        );
        assert!(links.peers.is_empty());
    }

    fn stamped(line: String, at: &str) -> String {
        let mut v: serde_json::Value = serde_json::from_str(&line).unwrap();
        v["timestamp"] = json!(at);
        v.to_string()
    }

    fn image(path: &str, touch: Touch, at_ms: Option<u64>) -> ImageFile {
        ImageFile {
            path: path.into(),
            touch,
            at_ms,
        }
    }

    #[test]
    fn an_image_file_read_or_written_is_listed_apart_from_the_files_with_when() {
        let got = read(
            &[
                stamped(
                    tool("t1", "Read", json!({"file_path": "/w/shot.png"})),
                    "2026-10-20T00:00:01.000Z",
                ),
                tool("t2", "Read", json!({"file_path": "/w/a.rs"})),
                stamped(
                    tool("t3", "Write", json!({"file_path": "plot.SVG"})),
                    "2026-10-20T00:00:02.500Z",
                ),
            ],
            &[],
        );
        let at = |ms: u64| Some(1_792_454_400_000 + ms);
        assert_eq!(
            got.images,
            [
                image("/w/plot.SVG", Touch::Edited, at(2_500)),
                image("/w/shot.png", Touch::Read, at(1_000)),
            ]
        );
        assert_eq!(got.files, [file("/w/a.rs", Touch::Read, None)]);
        assert!(got.rows().all(|l| l.target != "/w/shot.png"));
    }

    #[test]
    fn an_image_named_in_a_reply_is_listed_when_it_is_there_and_last_brought_up_counts() {
        let got = read(
            &[
                stamped(
                    tool("t1", "Read", json!({"file_path": "/w/a.jpg"})),
                    "2026-10-20T00:00:01.000Z",
                ),
                stamped(said("See a.jpg and gone.png"), "2026-10-20T00:00:09.000Z"),
            ],
            &["/w/a.jpg"],
        );
        assert_eq!(
            got.images,
            [image("/w/a.jpg", Touch::Read, Some(1_792_454_409_000))]
        );
    }

    #[test]
    fn an_image_is_told_by_its_extension_in_any_case() {
        for yes in [
            "a.png", "b.JPG", "c.jpeg", "d.gif", "e.webp", "f.bmp", "g.svg", "h.heic", "i.tiff",
        ] {
            assert!(is_image(yes), "{yes}");
        }
        for no in ["a.rs", "png", "/w/.png", "notes.md", "/w/dir.png/x"] {
            assert!(!is_image(no), "{no}");
        }
    }
}
