//! The sessions Pi keeps, one file a conversation: where they are, the
//! conversations they hold and the prompts sent in them.

use std::fs::File;
use std::io::{BufRead, BufReader, Read};
use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;

use adroit_model::{Agent, Past, title_of};
use serde_json::Value;

use crate::AgentContext;

/// How far into a session its name and first prompt are looked for.
const HEAD: u64 = 512 * 1024;

/// Where Pi, started as `context` says, keeps its sessions: in the
/// login's directory, else where the launch or adroit's own
/// environment moves them or Pi's state, else under `.pi/agent` in `home`.
pub(super) fn root(context: &AgentContext, home: &Path) -> PathBuf {
    if let Some(dir) = &context.home {
        return dir.join("sessions");
    }
    let set = |name: &str| {
        context
            .launch
            .env
            .get(name)
            .map(PathBuf::from)
            .or_else(|| std::env::var_os(name).map(PathBuf::from))
    };
    set("PI_CODING_AGENT_SESSION_DIR")
        .or_else(|| set("PI_CODING_AGENT_DIR").map(|dir| dir.join("sessions")))
        .unwrap_or_else(|| home.join(".pi").join("agent").join("sessions"))
}

/// Every session file under `root`: those Pi keeps in a folder for each
/// directory, and those kept in `root` itself, as a session folder Pi is
/// given holds them.
fn files(root: &Path) -> Vec<PathBuf> {
    let jsonl = |path: &Path| path.extension().is_some_and(|e| e == "jsonl");
    let mut found = Vec::new();
    for entry in std::fs::read_dir(root).into_iter().flatten().flatten() {
        let path = entry.path();
        match entry.file_type() {
            Ok(kind) if kind.is_file() && jsonl(&path) => found.push(path),
            Ok(kind) if kind.is_dir() => found.extend(
                std::fs::read_dir(&path)
                    .into_iter()
                    .flatten()
                    .flatten()
                    .map(|e| e.path())
                    .filter(|p| p.is_file() && jsonl(p)),
            ),
            _ => {}
        }
    }
    found
}

/// The file under `root` that holds the conversation `id`, which Pi names
/// `<when>_<id>.jsonl`.
pub(super) fn find(root: &Path, id: &str) -> Option<PathBuf> {
    let suffix = format!("_{id}.jsonl");
    files(root).into_iter().find(|path| {
        path.file_name()
            .and_then(|n| n.to_str())
            .is_some_and(|n| n.ends_with(&suffix))
    })
}

/// A session's header: its id and the directory it ran in.
fn header(line: &[u8]) -> Option<(String, PathBuf)> {
    let v: Value = serde_json::from_slice(line).ok()?;
    if v["type"] != "session" {
        return None;
    }
    Some((
        v["id"].as_str()?.to_string(),
        PathBuf::from(v["cwd"].as_str()?),
    ))
}

/// The text the user wrote, when `entry` records a message of theirs.
pub(super) fn typed(entry: &Value) -> Option<String> {
    if entry["type"] != "message" || entry["message"]["role"] != "user" {
        return None;
    }
    let content = &entry["message"]["content"];
    let text = match content.as_str() {
        Some(text) => text.to_string(),
        None => content
            .as_array()?
            .iter()
            .filter(|part| part["type"] == "text")
            .filter_map(|part| part["text"].as_str())
            .collect::<Vec<_>>()
            .join("\n"),
    };
    (!text.trim().is_empty()).then_some(text)
}

/// What the session at `path` holds, when it ran in `dir` or `dir` is none:
/// its id, directory, name, the prompts in it, and when it was last
/// written to; read no further than `limit` bytes in.
struct Kept {
    id: String,
    cwd: PathBuf,
    name: Option<String>,
    prompts: Vec<String>,
    at_ms: u64,
}

fn kept(path: &Path, dir: Option<&[PathBuf]>, limit: u64) -> Option<Kept> {
    let file = File::open(path).ok()?;
    let at_ms = file
        .metadata()
        .ok()?
        .modified()
        .ok()?
        .duration_since(UNIX_EPOCH)
        .ok()?
        .as_millis() as u64;
    let mut lines = BufReader::new(file.take(limit))
        .split(b'\n')
        .map_while(Result::ok);
    let (id, cwd) = header(&lines.next()?)?;
    if dir.is_some_and(|dir| !dir.contains(&cwd)) {
        return None;
    }
    let mut kept = Kept {
        id,
        cwd,
        name: None,
        prompts: Vec::new(),
        at_ms,
    };
    for line in lines {
        let Ok(entry) = serde_json::from_slice::<Value>(&line) else {
            continue;
        };
        if entry["type"] == "session_info" {
            kept.name = entry["name"]
                .as_str()
                .map(str::trim)
                .filter(|n| !n.is_empty())
                .map(String::from);
        } else if let Some(prompt) = typed(&entry) {
            kept.prompts.push(prompt);
        }
    }
    Some(kept)
}

/// `dir` as it is written and as Pi writes it, through the links it passes.
fn spellings(dir: &Path) -> Vec<PathBuf> {
    let mut spelt = vec![dir.to_path_buf()];
    spelt.extend(std::fs::canonicalize(dir).ok().filter(|real| real != dir));
    spelt
}

/// The conversations Pi kept under `root` that ran in `dir`, or in any
/// directory when none is given, those no one wrote a prompt in left out.
pub(super) fn past(root: &Path, dir: Option<&Path>) -> Vec<Past> {
    let dir = dir.map(spellings);
    files(root)
        .iter()
        .filter_map(|path| kept(path, dir.as_deref(), HEAD))
        .filter_map(|kept| {
            let title = kept.name.or_else(|| kept.prompts.into_iter().next())?;
            Some(Past {
                agent: Agent::Pi,
                title: title_of(&title),
                id: kept.id,
                cwd: kept.cwd,
                at_ms: kept.at_ms,
                ..Default::default()
            })
        })
        .collect()
}

/// The prompts sent in `project`, oldest first, from every conversation Pi
/// kept under `root` that ran there: Pi keeps no history of its own beside
/// them.
pub(super) fn history(root: &Path, project: &Path) -> Vec<String> {
    let project = spellings(project);
    let mut sessions: Vec<Kept> = files(root)
        .iter()
        .filter_map(|path| kept(path, Some(&project), u64::MAX))
        .collect();
    sessions.sort_by_key(|kept| kept.at_ms);
    sessions.into_iter().flat_map(|kept| kept.prompts).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A session file named as Pi names it, holding `lines` after its header.
    fn session(dir: &Path, id: &str, cwd: &str, lines: &[&str]) -> PathBuf {
        std::fs::create_dir_all(dir).unwrap();
        let path = dir.join(format!("2026-10-24T10-00-00-000Z_{id}.jsonl"));
        let mut text = format!(
            "{{\"type\":\"session\",\"version\":3,\"id\":\"{id}\",\"timestamp\":\"2026-10-24T10:00:00.000Z\",\"cwd\":\"{cwd}\"}}\n"
        );
        for line in lines {
            text.push_str(line);
            text.push('\n');
        }
        std::fs::write(&path, text).unwrap();
        path
    }

    fn user(text: &str) -> String {
        format!(
            r#"{{"type":"message","id":"a1","parentId":null,"timestamp":"2026-10-24T10:00:01.000Z","message":{{"role":"user","content":[{{"type":"text","text":"{text}"}}],"timestamp":1}}}}"#
        )
    }

    #[test]
    fn a_login_of_its_own_keeps_its_sessions_in_its_folder() {
        let context = AgentContext {
            home: Some("/w/pi".into()),
            ..AgentContext::builtin(Agent::Pi)
        };
        assert_eq!(
            root(&context, Path::new("/home")),
            Path::new("/w/pi/sessions")
        );
        let mut own = AgentContext::builtin(Agent::Pi);
        own.launch
            .env
            .insert("PI_CODING_AGENT_DIR".into(), "/elsewhere".into());
        assert_eq!(
            root(&own, Path::new("/home")),
            Path::new("/elsewhere/sessions")
        );
        own.launch
            .env
            .insert("PI_CODING_AGENT_SESSION_DIR".into(), "/s".into());
        assert_eq!(root(&own, Path::new("/home")), Path::new("/s"));
    }

    #[test]
    fn a_conversation_is_found_by_its_id_in_either_layout() {
        let dir = tempfile::tempdir().unwrap();
        let flat = session(dir.path(), "flat-1", "/r/a", &[]);
        let nested = session(&dir.path().join("--r-b--"), "nested-1", "/r/b", &[]);
        assert_eq!(find(dir.path(), "flat-1"), Some(flat));
        assert_eq!(find(dir.path(), "nested-1"), Some(nested));
        assert_eq!(find(dir.path(), "1"), None);
        assert_eq!(find(&dir.path().join("missing"), "flat-1"), None);
    }

    #[test]
    fn past_conversations_are_titled_by_their_name_else_their_first_prompt() {
        let dir = tempfile::tempdir().unwrap();
        let named = r#"{"type":"session_info","id":"b1","parentId":null,"timestamp":"t","name":"probe one"}"#;
        session(dir.path(), "named", "/r/a", &[named, &user("first")]);
        session(
            &dir.path().join("--r-a--"),
            "plain",
            "/r/a",
            &[&user("fix   the\\ntests")],
        );
        session(
            dir.path(),
            "silent",
            "/r/a",
            &[named.replace("probe one", "").as_str()],
        );
        session(dir.path(), "elsewhere", "/r/b", &[&user("other")]);
        let mut found = past(dir.path(), Some(Path::new("/r/a")));
        found.sort_by(|a, b| a.id.cmp(&b.id));
        let titles: Vec<(&str, &str)> = found
            .iter()
            .map(|p| (p.id.as_str(), p.title.as_str()))
            .collect();
        assert_eq!(titles, [("named", "probe one"), ("plain", "fix the tests")]);
        assert!(found.iter().all(|p| p.agent == Agent::Pi && p.at_ms > 0));
        assert_eq!(past(dir.path(), None).len(), 3);
    }

    #[test]
    fn a_directory_reached_through_a_link_is_the_one_pi_writes_through_it() {
        let dir = tempfile::tempdir().unwrap();
        let real = dir.path().join("real");
        std::fs::create_dir(&real).unwrap();
        let link = dir.path().join("link");
        std::os::unix::fs::symlink(&real, &link).unwrap();
        let written = std::fs::canonicalize(&real).unwrap();
        let root = dir.path().join("sessions");
        session(&root, "a", &written.display().to_string(), &[&user("one")]);
        assert_eq!(past(&root, Some(&link)).len(), 1);
        assert_eq!(history(&root, &link), ["one"]);
    }

    #[test]
    fn the_history_is_every_prompt_sent_in_the_project_oldest_first() {
        let dir = tempfile::tempdir().unwrap();
        let older = session(dir.path(), "a", "/r/a", &[&user("one"), &user("two")]);
        let file = File::options().write(true).open(&older).unwrap();
        file.set_modified(UNIX_EPOCH + std::time::Duration::from_secs(1))
            .unwrap();
        session(dir.path(), "b", "/r/a", &[&user("three")]);
        session(dir.path(), "c", "/r/b", &[&user("not here")]);
        assert_eq!(
            history(dir.path(), Path::new("/r/a")),
            ["one", "two", "three"]
        );
        let string = r#"{"type":"message","message":{"role":"user","content":"plain"}}"#;
        assert_eq!(
            typed(&serde_json::from_str(string).unwrap()).as_deref(),
            Some("plain")
        );
    }
}
