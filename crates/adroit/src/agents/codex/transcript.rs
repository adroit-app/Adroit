//! Read Codex rollout records into the overview's shared link collector.

use std::collections::HashSet;
use std::io::{Read, Seek, SeekFrom};
use std::os::unix::fs::MetadataExt;
use std::path::{Path, PathBuf};

use adroit_claude::links::{Collector, Links};
use serde_json::{Value, json};

use crate::agents::ConversationReader;

pub(super) fn thread_id(id: &str) -> bool {
    id.len() == 36
        && id.bytes().enumerate().all(|(i, b)| {
            if matches!(i, 8 | 13 | 18 | 23) {
                b == b'-'
            } else {
                b.is_ascii_hexdigit()
            }
        })
}

fn find(root: &Path, id: &str) -> Option<PathBuf> {
    if !thread_id(id) {
        return None;
    }
    fn inside(dir: &Path, suffix: &str, depth: usize) -> Option<PathBuf> {
        for entry in std::fs::read_dir(dir).ok()?.flatten() {
            let Ok(kind) = entry.file_type() else {
                continue;
            };
            if kind.is_file()
                && entry
                    .file_name()
                    .to_str()
                    .is_some_and(|n| n.starts_with("rollout-") && n.ends_with(suffix))
            {
                return Some(entry.path());
            }
            if kind.is_dir()
                && depth > 0
                && let Some(path) = inside(&entry.path(), suffix, depth - 1)
            {
                return Some(path);
            }
        }
        None
    }
    let suffix = format!("-{id}.jsonl");
    inside(&root.join("sessions"), &suffix, 3)
        .or_else(|| inside(&root.join("archived_sessions"), &suffix, 0))
}

pub(super) struct Transcript {
    path: PathBuf,
    locate: Option<(PathBuf, String)>,
    cwd: PathBuf,
    home: PathBuf,
    read_to: u64,
    inode: Option<u64>,
    rest: Vec<u8>,
    found: Collector,
    results: HashSet<String>,
}

impl Transcript {
    fn new(path: PathBuf, cwd: &Path, home: &Path) -> Self {
        Self {
            path,
            locate: None,
            cwd: cwd.into(),
            home: home.into(),
            read_to: 0,
            inode: None,
            rest: Vec::new(),
            found: Collector::new(cwd, home),
            results: HashSet::new(),
        }
    }

    pub(super) fn open(root: PathBuf, id: String, cwd: &Path, home: &Path) -> Self {
        let mut reader = Self::new(PathBuf::new(), cwd, home);
        reader.locate = Some((root, id));
        reader
    }

    fn restart(&mut self) {
        self.read_to = 0;
        self.inode = None;
        self.rest.clear();
        self.found = Collector::new(&self.cwd, &self.home);
        self.results.clear();
    }

    fn refresh(&mut self) {
        if !self.path.is_file()
            && let Some((root, id)) = &self.locate
        {
            self.path = find(root, id).unwrap_or_default();
        }
        let Ok(mut file) = std::fs::File::open(&self.path) else {
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
            if let Ok(value) = serde_json::from_str::<Value>(line) {
                self.take(&value);
            }
        }
    }

    // The shared collector owns address recognition, ordering and file labels.
    // Translate only conversation content, never reasoning or injected context.
    fn feed(&mut self, timestamp: &Value, role: &str, content: Value) {
        self.found.feed(
            &json!({"type":role, "timestamp":timestamp,
            "message":{"content":content}})
            .to_string(),
            &|p| p.is_file(),
        );
    }

    fn file(&mut self, timestamp: &Value, path: &str, edited: bool, cwd: &Path) {
        let path = local_path(path);
        let path = if path.starts_with("~/") {
            path
        } else {
            cwd.join(path).to_string_lossy().into_owned()
        };
        self.feed(
            timestamp,
            "assistant",
            json!([{"type":"tool_use",
            "name":if edited {"Edit"} else {"Read"}, "input":{"file_path":path}}]),
        );
    }

    fn take(&mut self, record: &Value) {
        let timestamp = &record["timestamp"];
        let payload = &record["payload"];
        match record["type"].as_str() {
            Some("response_item") => match payload["type"].as_str() {
                Some("message") => {
                    let Some(role @ ("assistant" | "user")) = payload["role"].as_str() else {
                        return;
                    };
                    if let Some(parts) = payload["content"].as_array() {
                        let parts: Vec<_> = parts
                            .iter()
                            .filter_map(|p| match p["type"].as_str() {
                                Some("input_text" | "output_text") => p["text"]
                                    .as_str()
                                    .map(|text| json!({"type":"text", "text":text})),
                                _ => None,
                            })
                            .collect();
                        self.feed(timestamp, role, json!(parts));
                    }
                }
                Some("function_call")
                    if payload["name"]
                        .as_str()
                        .is_some_and(|n| n == "view_image" || n == "functions.view_image") =>
                {
                    if let Some(args) = payload["arguments"]
                        .as_str()
                        .and_then(|s| serde_json::from_str::<Value>(s).ok())
                        && let Some(path) = args["path"].as_str()
                    {
                        self.file(timestamp, path, false, &self.cwd.clone());
                    }
                }
                Some("function_call") => {
                    let name = payload["name"].as_str().unwrap_or_default();
                    // Shell and filesystem results contain file contents, whose
                    // addresses are not part of the conversation's links.
                    if matches!(
                        name.rsplit('.').next().unwrap_or(name),
                        "exec"
                            | "exec_command"
                            | "shell"
                            | "shell_command"
                            | "write_stdin"
                            | "apply_patch"
                            | "read_file"
                            | "write_file"
                            | "edit_file"
                            | "grep"
                            | "list_directory"
                            | "search_files"
                    ) {
                        return;
                    }
                    if let Some(input) = payload["arguments"]
                        .as_str()
                        .and_then(|s| serde_json::from_str::<Value>(s).ok())
                    {
                        let id = &payload["call_id"];
                        self.feed(
                            timestamp,
                            "assistant",
                            json!([{"type":"tool_use", "id":id,
                            "name":name, "input":input}]),
                        );
                        if let Some(id) = id.as_str() {
                            self.results.insert(id.into());
                        }
                    }
                }
                Some("function_call_output") => {
                    if let Some(id) = payload["call_id"].as_str()
                        && self.results.remove(id)
                    {
                        self.feed(
                            timestamp,
                            "user",
                            json!([{"type":"tool_result", "tool_use_id":id,
                            "content":payload["output"]}]),
                        );
                    }
                }
                _ => {}
            },
            Some("event_msg") => {
                let event = if payload["type"] == "item_completed" {
                    &payload["item"]
                } else {
                    payload
                };
                match event["type"].as_str() {
                    Some("exec_command_end" | "CommandExecution") if event["exit_code"] == 0 => {
                        let cwd = event["cwd"]
                            .as_str()
                            .map(local_path)
                            .map(PathBuf::from)
                            .unwrap_or_else(|| self.cwd.clone());
                        if let Some(parsed) = event["parsed_cmd"].as_array() {
                            for cmd in parsed {
                                if cmd["type"] == "read"
                                    && let Some(path) = cmd["path"].as_str()
                                {
                                    self.file(timestamp, path, false, &cwd);
                                }
                            }
                        }
                    }
                    Some("view_image_tool_call" | "ImageView") => {
                        if let Some(path) = event["path"].as_str() {
                            self.file(timestamp, path, false, &self.cwd.clone());
                        }
                    }
                    Some("ImageGeneration") if event["status"] == "completed" => {
                        if let Some(path) = event["saved_path"].as_str() {
                            self.file(timestamp, path, true, &self.cwd.clone());
                        }
                    }
                    Some("WebSearch") => {
                        self.feed(
                            timestamp,
                            "user",
                            json!([{"type":"tool_result",
                            "content":event.to_string()}]),
                        );
                    }
                    Some("patch_apply_end" | "FileChange")
                        if event["success"] == true || event["status"] == "completed" =>
                    {
                        if let Some(changes) = event["changes"].as_object() {
                            for (path, change) in changes {
                                self.file(timestamp, path, true, &self.cwd.clone());
                                if let Some(moved) = change["move_path"].as_str() {
                                    self.file(timestamp, moved, true, &self.cwd.clone());
                                }
                            }
                        }
                    }
                    _ => {}
                }
            }
            _ => {}
        }
    }
}

fn local_path(path: &str) -> String {
    let Some(path) = path.strip_prefix("file://") else {
        return path.into();
    };
    let mut bytes = Vec::new();
    let mut rest = path.as_bytes();
    while let Some((&first, tail)) = rest.split_first() {
        if first == b'%'
            && rest.len() >= 3
            && let (Some(a), Some(b)) = (
                (rest[1] as char).to_digit(16),
                (rest[2] as char).to_digit(16),
            )
        {
            bytes.push((a * 16 + b) as u8);
            rest = &rest[3..];
        } else {
            bytes.push(first);
            rest = tail;
        }
    }
    String::from_utf8(bytes).unwrap_or_else(|_| path.into())
}

impl ConversationReader for Transcript {
    fn read(&mut self) -> Links {
        self.refresh();
        self.found.links()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use adroit_claude::links::{Kind, Touch};
    use serde_json::{Value, json};
    use std::io::Write;

    fn record(kind: &str, payload: Value) -> String {
        format!(
            "{}\n",
            json!({"timestamp":"2026-10-20T12:00:00Z", "type":kind, "payload":payload})
        )
    }

    fn message(role: &str, text: &str) -> String {
        record(
            "response_item",
            json!({"type":"message", "role":role,
            "content":[{"type":"output_text", "text":text}]}),
        )
    }

    #[test]
    fn replies_supply_artifacts_links_and_existing_files_but_instructions_do_not() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("rollout.jsonl");
        std::fs::write(dir.path().join("report.md"), "report").unwrap();
        std::fs::write(
            &path,
            [
                message("developer", "https://instructions.example"),
                record(
                    "response_item",
                    json!({"type":"reasoning", "text":"https://reasoning.example"}),
                ),
                message("user", "See https://user.example"),
                message(
                    "assistant",
                    "[Report](report.md:3) https://docs.example https://claude.ai/artifact/example",
                ),
            ]
            .concat(),
        )
        .unwrap();
        let mut reader = Transcript::new(path, dir.path(), dir.path());
        let links = reader.read();
        assert_eq!(links.artifacts.len(), 1);
        assert_eq!(
            links
                .urls
                .iter()
                .map(|l| l.target.as_str())
                .collect::<Vec<_>>(),
            ["https://docs.example", "https://user.example"]
        );
        assert_eq!(links.files.len(), 1);
        assert_eq!(links.files[0].line, Some(3));
        assert_eq!(links.files[0].kind, Kind::File(Touch::Mentioned));
    }

    #[test]
    fn completed_tools_supply_images_reads_and_edits_without_indexing_file_contents() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("rollout.jsonl");
        std::fs::write(
            &path,
            [
                record(
                    "event_msg",
                    json!({"type":"exec_command_end", "exit_code":0,
                "cwd":dir.path(), "parsed_cmd":[{"type":"read", "path":"source.rs"}],
                "aggregated_output":"https://inside-file.example"}),
                ),
                record(
                    "event_msg",
                    json!({"type":"view_image_tool_call", "path":dir.path().join("shot.png")}),
                ),
                record(
                    "event_msg",
                    json!({"type":"patch_apply_end", "success":false,
                "changes":{"failed.rs":{"type":"add"}}}),
                ),
                record(
                    "event_msg",
                    json!({"type":"patch_apply_end", "success":true,
                "changes":{"source.rs":{"type":"update"}}}),
                ),
                record(
                    "event_msg",
                    json!({"type":"item_completed", "item":{
                "type":"CommandExecution", "exit_code":0,
                "cwd":format!("file://{}",dir.path().display()),
                "parsed_cmd":[{"type":"read", "path":"other.rs"}]}}),
                ),
                record(
                    "event_msg",
                    json!({"type":"item_completed", "item":{
                "type":"FileChange", "status":"completed",
                "changes":{"old.rs":{"type":"update", "move_path":"new.rs"}}}}),
                ),
            ]
            .concat(),
        )
        .unwrap();
        let mut reader = Transcript::new(path, dir.path(), dir.path());
        let links = reader.read();
        assert!(links.urls.is_empty());
        assert_eq!(links.images.len(), 1);
        assert_eq!(links.images[0].touch, Touch::Read);
        assert!(links.images[0].at_ms.is_some());
        let files: Vec<_> = links
            .files
            .iter()
            .map(|l| {
                (
                    Path::new(&l.target).file_name().unwrap().to_str().unwrap(),
                    l.kind,
                )
            })
            .collect();
        assert!(files.contains(&("source.rs", Kind::File(Touch::Edited))));
        assert!(files.contains(&("other.rs", Kind::File(Touch::Read))));
        assert!(files.contains(&("new.rs", Kind::File(Touch::Edited))));
        assert!(!files.iter().any(|(p, _)| *p == "failed.rs"));
    }

    #[test]
    fn reads_follow_appends_complete_lines_replacement_and_removal() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("rollout.jsonl");
        let mut reader = Transcript::new(path.clone(), dir.path(), dir.path());
        assert!(reader.read().is_empty());
        let first = message("assistant", "https://first.example");
        std::fs::write(&path, first.trim_end()).unwrap();
        assert!(reader.read().is_empty());
        let mut file = std::fs::OpenOptions::new()
            .append(true)
            .open(&path)
            .unwrap();
        writeln!(file).unwrap();
        assert_eq!(reader.read().urls.len(), 1);
        assert_eq!(reader.read().urls.len(), 1);
        file.write_all(message("assistant", "https://second.example").as_bytes())
            .unwrap();
        assert_eq!(reader.read().urls[0].target, "https://second.example");
        let replacement = dir.path().join("replacement");
        std::fs::write(
            &replacement,
            message("assistant", "https://replacement.example"),
        )
        .unwrap();
        std::fs::rename(replacement, &path).unwrap();
        assert_eq!(reader.read().urls.len(), 1);
        assert_eq!(reader.read().urls[0].target, "https://replacement.example");
        std::fs::remove_file(path).unwrap();
        assert!(reader.read().is_empty());
    }

    #[test]
    fn discovery_matches_the_full_thread_id_in_active_and_archived_rollouts() {
        let dir = tempfile::tempdir().unwrap();
        let id = "01234567-89ab-cdef-0123-456789abcdef";
        let folder = dir.path().join("sessions/2026/10/20");
        std::fs::create_dir_all(&folder).unwrap();
        let path = folder.join(format!("rollout-2026-10-20T12-00-00-{id}.jsonl"));
        std::fs::write(&path, "").unwrap();
        assert_eq!(find(dir.path(), id), Some(path.clone()));
        assert_eq!(find(dir.path(), "abcdef"), None);
        assert_eq!(find(dir.path(), ""), None);
        let archive = dir.path().join("archived_sessions");
        std::fs::create_dir(&archive).unwrap();
        let archived = archive.join(path.file_name().unwrap());
        std::fs::rename(path, &archived).unwrap();
        assert_eq!(find(dir.path(), id), Some(archived));
    }

    #[test]
    fn a_reader_waits_for_its_own_rollout_and_follows_it_into_the_archive() {
        let dir = tempfile::tempdir().unwrap();
        let id = "01234567-89ab-cdef-0123-456789abcdef";
        let mut reader = Transcript::open(dir.path().into(), id.into(), dir.path(), dir.path());
        assert!(reader.read().is_empty());
        let folder = dir.path().join("sessions/2026/10/20");
        std::fs::create_dir_all(&folder).unwrap();
        let path = folder.join(format!("rollout-now-{id}.jsonl"));
        std::fs::write(&path, message("assistant", "https://own.example")).unwrap();
        std::fs::write(
            folder.join("rollout-other.jsonl"),
            message("assistant", "https://other.example"),
        )
        .unwrap();
        assert_eq!(reader.read().urls[0].target, "https://own.example");
        let archive = dir.path().join("archived_sessions");
        std::fs::create_dir(&archive).unwrap();
        std::fs::rename(&path, archive.join(path.file_name().unwrap())).unwrap();
        assert_eq!(reader.read().urls.len(), 1);
        assert_eq!(reader.read().urls[0].target, "https://own.example");
    }

    #[test]
    fn file_uris_decode_escaped_paths_without_changing_literal_percent_names() {
        assert_eq!(
            local_path("file:///tmp/my%20file%E3%81%82.png"),
            "/tmp/my fileあ.png"
        );
        assert_eq!(local_path("/tmp/my%20file.png"), "/tmp/my%20file.png");
    }

    #[test]
    fn web_tool_links_are_collected_but_shell_commands_and_their_output_are_not() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("rollout.jsonl");
        std::fs::write(
            &path,
            [
                record(
                    "response_item",
                    json!({"type":"function_call", "name":"web.run", "call_id":"web",
                "arguments":json!({"open":[{"ref_id":"https://requested.example"}]}).to_string()}),
                ),
                record(
                    "response_item",
                    json!({"type":"function_call_output", "call_id":"web",
                "output":"Read https://result.example"}),
                ),
                record(
                    "response_item",
                    json!({"type":"function_call", "name":"exec_command", "call_id":"shell",
                "arguments":json!({"cmd":"echo https://written.example > file.txt"}).to_string()}),
                ),
                record(
                    "response_item",
                    json!({"type":"function_call_output", "call_id":"shell",
                "output":"https://file-content.example"}),
                ),
            ]
            .concat(),
        )
        .unwrap();
        let mut reader = Transcript::new(path, dir.path(), dir.path());
        assert_eq!(
            reader
                .read()
                .urls
                .iter()
                .map(|l| l.target.as_str())
                .collect::<Vec<_>>(),
            ["https://result.example", "https://requested.example"]
        );
    }

    #[test]
    fn current_image_and_hosted_search_events_supply_their_outputs() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("rollout.jsonl");
        std::fs::write(&path, [
            record("event_msg", json!({"type":"item_completed", "item":{
                "type":"ImageView", "path":format!("file://{}/view.png", dir.path().display())}})),
            record("event_msg", json!({"type":"item_completed", "item":{
                "type":"ImageGeneration", "status":"completed", "saved_path":dir.path().join("made.png")}})),
            record("event_msg", json!({"type":"item_completed", "item":{
                "type":"WebSearch", "action":{"type":"open_page", "url":"https://opened.example"}}})),
        ].concat()).unwrap();
        let mut reader = Transcript::new(path, dir.path(), dir.path());
        let links = reader.read();
        assert_eq!(links.images.len(), 2);
        assert_eq!(links.images[0].touch, Touch::Edited);
        assert_eq!(links.images[1].touch, Touch::Read);
        assert_eq!(links.urls[0].target, "https://opened.example");
    }
}
