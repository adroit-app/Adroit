//! The conversations Codex kept, read from its rollout records: one file per
//! conversation, in a folder per day, and the names it gave them in an index.

use std::collections::HashMap;
use std::fs::File;
use std::io::{BufRead, BufReader, Read};
use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;

use adroit_model::{Agent, Past, title_of};
use serde_json::Value;

/// How far into a record its first prompt is looked for.
const HEAD: u64 = 512 * 1024;

/// What started a conversation that a person had in Codex's interface,
/// rather than one Codex ran on its own or for a program.
const INTERACTIVE: [&str; 2] = ["cli", "vscode"];

/// The conversations Codex kept under `root` that ran in `dir`, or in any
/// directory when none is given.
pub(super) fn read(root: &Path, dir: Option<&Path>) -> Vec<Past> {
    let names = names(&root.join("session_index.jsonl"));
    let mut files = Vec::new();
    collect(&root.join("sessions"), 3, &mut files);
    files
        .iter()
        .filter_map(|path| conversation(path, dir, &names))
        .collect()
}

/// The rollout records in `dir`, `depth` folders deep.
fn collect(dir: &Path, depth: usize, out: &mut Vec<PathBuf>) {
    for entry in std::fs::read_dir(dir).into_iter().flatten().flatten() {
        let path = entry.path();
        match entry.file_type() {
            Ok(kind) if kind.is_dir() && depth > 0 => collect(&path, depth - 1, out),
            Ok(kind)
                if kind.is_file()
                    && path
                        .file_name()
                        .and_then(|n| n.to_str())
                        .is_some_and(|n| n.starts_with("rollout-") && n.ends_with(".jsonl")) =>
            {
                out.push(path)
            }
            _ => {}
        }
    }
}

/// The name Codex last gave each conversation, by its id.
fn names(index: &Path) -> HashMap<String, String> {
    let Ok(text) = std::fs::read_to_string(index) else {
        return HashMap::new();
    };
    text.lines()
        .filter_map(|l| serde_json::from_str::<Value>(l).ok())
        .filter_map(|v| {
            let id = v["id"].as_str()?.to_string();
            let name = v["thread_name"].as_str()?.trim();
            (!name.is_empty()).then(|| (id, name.to_string()))
        })
        .collect()
}

/// The conversation the record at `path` holds, when it ran in `dir` and a
/// person wrote in it.
fn conversation(path: &Path, dir: Option<&Path>, names: &HashMap<String, String>) -> Option<Past> {
    let file = File::open(path).ok()?;
    let at_ms = file
        .metadata()
        .ok()?
        .modified()
        .ok()?
        .duration_since(UNIX_EPOCH)
        .ok()?
        .as_millis() as u64;
    let mut lines = BufReader::new(file.take(HEAD))
        .split(b'\n')
        .map_while(Result::ok);
    let meta: Value = serde_json::from_slice(&lines.next()?).ok()?;
    if meta["type"] != "session_meta" {
        return None;
    }
    let meta = &meta["payload"];
    if let Some(source) = meta.get("source")
        && !source.as_str().is_some_and(|s| INTERACTIVE.contains(&s))
    {
        return None;
    }
    let id = meta["id"].as_str()?.to_string();
    let cwd = PathBuf::from(meta["cwd"].as_str()?);
    if dir.is_some_and(|dir| dir != cwd) {
        return None;
    }
    let prompt = lines.find_map(|line| typed(&line));
    let title = names.get(&id).cloned().or(prompt)?;
    Some(Past {
        agent: Agent::Codex,
        title: title_of(&title),
        id,
        cwd,
        at_ms,
        ..Default::default()
    })
}

/// What the user typed, when `line` records a message of theirs. What Codex
/// adds in their name, the instructions it read and its context among it,
/// is not.
fn typed(line: &[u8]) -> Option<String> {
    let v: Value = serde_json::from_slice(line).ok()?;
    let p = &v["payload"];
    let text = match (v["type"].as_str()?, p["type"].as_str()?) {
        ("response_item", "message") if p["role"] == "user" => p["content"]
            .as_array()?
            .iter()
            .find(|c| c["type"] == "input_text")?["text"]
            .as_str()?,
        ("event_msg", "user_message") => p["message"].as_str()?,
        _ => return None,
    };
    let text = text.trim();
    (!text.is_empty() && !text.starts_with('<') && !text.starts_with("# AGENTS.md"))
        .then(|| text.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    const ID: &str = "019f7a4b-fee9-74d3-9977-5bc9bb134d11";

    fn meta(id: &str, cwd: &str, source: &str) -> String {
        json!({"type": "session_meta", "payload": {"id": id, "cwd": cwd, "source": source,
            "base_instructions": {"text": "You are Codex"}}})
        .to_string()
    }

    fn said(text: &str) -> String {
        json!({"type": "response_item", "payload": {"type": "message", "role": "user",
            "content": [{"type": "input_text", "text": text}]}})
        .to_string()
    }

    fn write(root: &Path, day: &str, id: &str, lines: &[String]) {
        let folder = root.join("sessions").join(day);
        std::fs::create_dir_all(&folder).unwrap();
        let path = folder.join(format!("rollout-2026-10-22T02-03-13-{id}.jsonl"));
        std::fs::write(path, lines.join("\n") + "\n").unwrap();
    }

    #[test]
    fn a_conversation_is_named_by_its_first_prompt_past_what_codex_adds() {
        let root = tempfile::tempdir().unwrap();
        write(
            root.path(),
            "2026/10/22",
            ID,
            &[
                meta(ID, "/src/api", "cli"),
                said("# AGENTS.md instructions for /src/api\n\n<INSTRUCTIONS>"),
                said("<environment_context>\n</environment_context>"),
                said("review\nthe API"),
                said("and the tests"),
            ],
        );
        let got = read(root.path(), Some(Path::new("/src/api")));
        assert_eq!(got.len(), 1);
        assert_eq!(got[0].agent, Agent::Codex);
        assert_eq!(got[0].id, ID);
        assert_eq!(got[0].cwd, PathBuf::from("/src/api"));
        assert_eq!(got[0].title, "review the API");
    }

    #[test]
    fn a_prompt_codex_recorded_as_an_event_names_it_too() {
        let root = tempfile::tempdir().unwrap();
        let event = json!({"type": "event_msg",
            "payload": {"type": "user_message", "message": "fix it"}});
        write(
            root.path(),
            "2026/10/22",
            ID,
            &[meta(ID, "/r", "cli"), event.to_string()],
        );
        assert_eq!(read(root.path(), None)[0].title, "fix it");
    }

    #[test]
    fn the_name_codex_gave_a_conversation_last_wins_over_its_prompt() {
        let root = tempfile::tempdir().unwrap();
        write(
            root.path(),
            "2026/10/22",
            ID,
            &[meta(ID, "/r", "cli"), said("hello")],
        );
        let index = [
            json!({"id": ID, "thread_name": "first", "updated_at": "2026-10-22T00:00:00Z"}),
            json!({"id": ID, "thread_name": "Weekday brief", "updated_at": "2026-10-23T00:00:00Z"}),
        ];
        let index: Vec<String> = index.iter().map(Value::to_string).collect();
        std::fs::write(root.path().join("session_index.jsonl"), index.join("\n")).unwrap();
        assert_eq!(read(root.path(), None)[0].title, "Weekday brief");
    }

    #[test]
    fn a_conversation_codex_ran_on_its_own_or_no_one_wrote_in_is_not_listed() {
        let root = tempfile::tempdir().unwrap();
        let other = "019f7a4f-97e0-79a0-9bc1-ebe561c2bef1";
        write(
            root.path(),
            "2026/10/22",
            ID,
            &[meta(ID, "/r", "exec"), said("run the job")],
        );
        write(
            root.path(),
            "2026/10/23",
            other,
            &[meta(other, "/r", "cli")],
        );
        assert!(read(root.path(), None).is_empty());
    }

    #[test]
    fn a_directory_s_conversations_leave_out_every_other_s() {
        let root = tempfile::tempdir().unwrap();
        let other = "019f7a4f-97e0-79a0-9bc1-ebe561c2bef1";
        write(
            root.path(),
            "2026/10/22",
            ID,
            &[meta(ID, "/a", "cli"), said("one")],
        );
        write(
            root.path(),
            "2026/10/23",
            other,
            &[meta(other, "/b", "vscode"), said("two")],
        );
        let got = read(root.path(), Some(Path::new("/a")));
        let ids: Vec<&str> = got.iter().map(|p| p.id.as_str()).collect();
        assert_eq!(ids, [ID]);
        assert_eq!(read(root.path(), None).len(), 2);
    }

    #[test]
    fn nothing_is_listed_where_codex_kept_nothing() {
        let root = tempfile::tempdir().unwrap();
        assert!(read(root.path(), None).is_empty());
    }
}
