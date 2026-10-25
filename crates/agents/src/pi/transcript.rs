//! Read a Pi session into the overview's shared link collector, and count
//! what it did, as the file grows.

use std::collections::HashSet;
use std::io::{Read, Seek, SeekFrom};
use std::os::unix::fs::MetadataExt;
use std::path::{Path, PathBuf};

use adroit_model::{ConversationStats, Tokens};
use serde_json::{Value, json};

use crate::ConversationReader;
use crate::links::{Collector, Links};
use crate::time::timestamp_ms;

/// The tools whose calls name no link of the conversation's own: what they
/// find is the contents of files and of the shell.
const SEARCHING: [&str; 4] = ["bash", "grep", "find", "ls"];

pub(super) struct Transcript {
    root: PathBuf,
    id: String,
    path: Option<PathBuf>,
    cwd: PathBuf,
    home: PathBuf,
    read_to: u64,
    inode: Option<u64>,
    rest: Vec<u8>,
    found: Collector,
    /// The calls fed to the collector, whose results it is given too.
    calls: HashSet<String>,
    stats: ConversationStats,
}

impl Transcript {
    /// The reader of the conversation `id`, kept under `root`, of a session
    /// in `cwd`, `~` standing for `home`.
    pub(super) fn open(root: PathBuf, id: String, cwd: &Path, home: &Path) -> Self {
        Self {
            root,
            id,
            path: None,
            cwd: cwd.into(),
            home: home.into(),
            read_to: 0,
            inode: None,
            rest: Vec::new(),
            found: Collector::new(cwd, home),
            calls: HashSet::new(),
            stats: ConversationStats::default(),
        }
    }

    fn restart(&mut self) {
        self.read_to = 0;
        self.inode = None;
        self.rest.clear();
        self.found = Collector::new(&self.cwd, &self.home);
        self.calls.clear();
        self.stats = ConversationStats::default();
    }

    fn refresh(&mut self) {
        if !self.path.as_ref().is_some_and(|p| p.is_file()) {
            self.path = super::sessions::find(&self.root, &self.id);
        }
        let Some(Ok(mut file)) = self.path.as_ref().map(std::fs::File::open) else {
            self.restart();
            return;
        };
        let Ok(meta) = file.metadata() else { return };
        if meta.len() < self.read_to || self.inode.is_some_and(|ino| ino != meta.ino()) {
            self.restart();
        }
        self.inode = Some(meta.ino());
        let mut bytes = Vec::new();
        if file
            .seek(SeekFrom::Start(self.read_to))
            .and_then(|_| file.take(meta.len() - self.read_to).read_to_end(&mut bytes))
            .is_err()
        {
            return;
        }
        self.read_to += bytes.len() as u64;
        self.rest.extend(bytes);
        let Some(end) = self.rest.iter().rposition(|b| *b == b'\n') else {
            return;
        };
        let done: Vec<u8> = self.rest.drain(..=end).collect();
        for line in String::from_utf8_lossy(&done).lines() {
            if let Ok(entry) = serde_json::from_str::<Value>(line) {
                self.take(&entry);
            }
        }
    }

    // The shared collector owns address recognition, ordering and file
    // labels: Pi's messages are put to it in the shape it reads.
    fn feed(&mut self, timestamp: &Value, role: &str, content: Value) {
        self.found.feed(
            &json!({"type":role, "timestamp":timestamp, "message":{"content":content}}).to_string(),
            &|p| p.is_file(),
        );
    }

    fn take(&mut self, entry: &Value) {
        if entry["type"] != "message" {
            return;
        }
        let timestamp = &entry["timestamp"];
        let message = &entry["message"];
        match message["role"].as_str() {
            Some("user") => {
                self.stats.user_turns += 1;
                self.stats.last_user_ms = self.stats.last_user_ms.max(timestamp_ms(timestamp));
                let content = match &message["content"] {
                    Value::String(text) => json!([{"type":"text", "text":text}]),
                    parts => texts(parts),
                };
                self.feed(timestamp, "user", content);
            }
            Some("assistant") => {
                self.count(message);
                let mut content = Vec::new();
                for part in message["content"].as_array().into_iter().flatten() {
                    match part["type"].as_str() {
                        Some("text") => content.push(json!({"type":"text", "text":part["text"]})),
                        Some("toolCall") => {
                            if let Some(call) = self.call(part) {
                                content.push(call);
                            }
                        }
                        _ => {}
                    }
                }
                self.feed(timestamp, "assistant", json!(content));
            }
            Some("toolResult") => {
                if let Some(id) = message["toolCallId"].as_str()
                    && self.calls.remove(id)
                {
                    self.feed(
                        timestamp,
                        "user",
                        json!([{"type":"tool_result", "tool_use_id":id,
                            "content":texts(&message["content"])}]),
                    );
                }
            }
            _ => {}
        }
    }

    /// The call `part` as the collector reads one: Pi's file tools as the
    /// ones it knows, by the file's whole path; none for a search.
    fn call(&mut self, part: &Value) -> Option<Value> {
        self.stats.tool_calls += 1;
        let name = part["name"].as_str()?;
        let id = part["id"].as_str()?;
        if SEARCHING.contains(&name) {
            return None;
        }
        let arguments = &part["arguments"];
        let call = match (name, arguments["path"].as_str()) {
            ("read" | "edit" | "write", Some(path)) => {
                let name = match name {
                    "read" => "Read",
                    "edit" => "Edit",
                    _ => "Write",
                };
                json!({"type":"tool_use", "id":id, "name":name,
                    "input":{"file_path":self.whole(path)}})
            }
            _ => json!({"type":"tool_use", "id":id, "name":name, "input":arguments}),
        };
        self.calls.insert(id.into());
        Some(call)
    }

    fn whole(&self, path: &str) -> String {
        match path.strip_prefix("~/") {
            Some(rest) => self.home.join(rest),
            None => self.cwd.join(path),
        }
        .to_string_lossy()
        .into_owned()
    }

    fn count(&mut self, message: &Value) {
        if let Some(model) = message["model"].as_str().filter(|m| !m.is_empty()) {
            self.stats.model = Some(model.into());
        }
        let usage = &message["usage"];
        if usage.is_object() {
            let n = |key: &str| usage[key].as_u64().unwrap_or(0);
            let tokens = self.stats.tokens.get_or_insert_with(Tokens::default);
            tokens.input += n("input") + n("cacheRead") + n("cacheWrite");
            tokens.output += n("output");
            tokens.cached += n("cacheRead");
        }
    }
}

/// The text parts of `content`.
fn texts(content: &Value) -> Value {
    json!(
        content
            .as_array()
            .into_iter()
            .flatten()
            .filter(|part| part["type"] == "text")
            .map(|part| json!({"type":"text", "text":part["text"]}))
            .collect::<Vec<_>>()
    )
}

impl ConversationReader for Transcript {
    fn stats(&self) -> ConversationStats {
        self.stats.clone()
    }

    fn read(&mut self) -> Links {
        self.refresh();
        self.found.links()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::links::{Kind, Touch};
    use crate::testing::append;

    fn entry(message: Value) -> String {
        format!(
            "{}\n",
            json!({"type":"message", "id":"e", "parentId":null,
                "timestamp":"2026-10-24T10:00:00.000Z", "message":message})
        )
    }

    /// A reader of the conversation `id`, its file under `root` holding the
    /// header alone.
    fn reader(root: &Path, cwd: &Path, id: &str) -> (Transcript, PathBuf) {
        let path = root.join(format!("2026-10-24T10-00-00-000Z_{id}.jsonl"));
        std::fs::write(
            &path,
            format!(
                "{}\n",
                json!({"type":"session","version":3,"id":id,"cwd":cwd})
            ),
        )
        .unwrap();
        (Transcript::open(root.into(), id.into(), cwd, root), path)
    }

    #[test]
    fn replies_and_prompts_supply_links_and_the_conversation_is_counted() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("report.md"), "report").unwrap();
        let (mut reader, path) = reader(dir.path(), dir.path(), "c1");
        append(
            &path,
            &[
                entry(json!({"role":"system", "content":"https://system.example"})),
                entry(json!({"role":"user", "content":[{"type":"text", "text":"see https://user.example"}]})),
                entry(json!({"role":"assistant", "model":"slow", "provider":"fake",
                    "usage":{"input":10, "output":6, "cacheRead":4, "cacheWrite":0},
                    "content":[{"type":"thinking", "thinking":"https://thought.example"},
                        {"type":"text", "text":"[Report](report.md:3) https://docs.example"}]})),
            ]
            .concat(),
        );
        let links = reader.read();
        let urls: Vec<&str> = links.urls.iter().map(|l| l.target.as_str()).collect();
        assert_eq!(urls, ["https://docs.example", "https://user.example"]);
        assert_eq!(links.files.len(), 1);
        assert_eq!(links.files[0].line, Some(3));
        let stats = reader.stats();
        assert_eq!(stats.model.as_deref(), Some("slow"));
        assert_eq!(stats.user_turns, 1);
        assert_eq!(
            stats.tokens,
            Some(Tokens {
                input: 14,
                output: 6,
                cached: 4
            })
        );
    }

    #[test]
    fn file_tools_supply_reads_and_edits_and_searches_supply_nothing() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("src.rs"), "fn main() {}").unwrap();
        std::fs::write(dir.path().join("out.rs"), "").unwrap();
        let (mut reader, path) = reader(dir.path(), dir.path(), "c2");
        append(
            &path,
            &[
                entry(json!({"role":"assistant", "content":[
                    {"type":"toolCall", "id":"t1", "name":"read", "arguments":{"path":"src.rs"}},
                    {"type":"toolCall", "id":"t2", "name":"edit", "arguments":{"path":"out.rs", "edits":[]}},
                    {"type":"toolCall", "id":"t3", "name":"bash", "arguments":{"command":"curl https://shell.example"}}]})),
                entry(json!({"role":"toolResult", "toolCallId":"t1", "toolName":"read",
                    "content":[{"type":"text", "text":"https://inside-file.example"}]})),
                entry(json!({"role":"toolResult", "toolCallId":"t3", "toolName":"bash",
                    "content":[{"type":"text", "text":"https://shell-output.example"}]})),
            ]
            .concat(),
        );
        let links = reader.read();
        assert!(links.urls.is_empty(), "{:?}", links.urls);
        let touched: Vec<(String, Kind)> = links
            .files
            .iter()
            .map(|l| (l.target.clone(), l.kind))
            .collect();
        assert!(touched.contains(&(
            dir.path().join("src.rs").display().to_string(),
            Kind::File(Touch::Read)
        )));
        assert!(touched.contains(&(
            dir.path().join("out.rs").display().to_string(),
            Kind::File(Touch::Edited)
        )));
        assert_eq!(reader.stats().tool_calls, 3);
    }

    #[test]
    fn the_file_is_read_as_it_grows_and_again_when_it_is_replaced() {
        let dir = tempfile::tempdir().unwrap();
        let (mut reader, path) = reader(dir.path(), dir.path(), "c3");
        assert!(reader.read().urls.is_empty());
        append(
            &path,
            &entry(json!({"role":"user", "content":"https://first.example"})),
        );
        assert_eq!(reader.read().urls.len(), 1);
        append(
            &path,
            "{\"type\":\"message\",\"message\":{\"role\":\"user\"",
        );
        assert_eq!(reader.read().urls.len(), 1);
        std::fs::remove_file(&path).unwrap();
        let (_, path) = super::tests::reader(dir.path(), dir.path(), "c3");
        append(
            &path,
            &entry(json!({"role":"user", "content":"https://second.example"})),
        );
        let urls: Vec<String> = reader.read().urls.into_iter().map(|l| l.target).collect();
        assert_eq!(urls, ["https://second.example"]);
    }
}
