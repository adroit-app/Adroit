//! The conversations Claude Code kept, read from its transcripts: one file
//! per conversation, in a folder per directory.

use std::fs::File;
use std::io::{BufRead, BufReader, Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;

use adroit_model::{Agent, Past, title_of};
use serde_json::Value;

/// How far into a transcript its directory and first prompt are looked for.
const HEAD: u64 = 256 * 1024;
/// How much of a transcript's end is read for the name it was last given.
const TAIL: u64 = 32 * 1024;

/// The conversations Claude Code kept under `projects` that ran in `dir`, or
/// in any directory when none is given.
pub(crate) fn read(projects: &Path, dir: Option<&Path>) -> Vec<Past> {
    let folders = match dir {
        Some(dir) => vec![projects.join(super::places::transcripts_name(dir))],
        None => entries(projects).filter(|p| p.is_dir()).collect(),
    };
    folders
        .iter()
        .flat_map(|folder| entries(folder))
        .filter(|p| p.extension().is_some_and(|e| e == "jsonl"))
        .filter_map(|p| conversation(&p))
        // The folder's name leaves out what the path's punctuation was.
        .filter(|p| dir.is_none_or(|dir| p.cwd == dir))
        .collect()
}

fn entries(dir: &Path) -> impl Iterator<Item = PathBuf> {
    std::fs::read_dir(dir)
        .into_iter()
        .flatten()
        .flatten()
        .map(|e| e.path())
}

/// The conversation the transcript at `path` holds; none for one no prompt
/// was written in.
fn conversation(path: &Path) -> Option<Past> {
    let id = path.file_stem()?.to_str()?.to_string();
    let mut file = File::open(path).ok()?;
    let meta = file.metadata().ok()?;
    let at_ms = meta
        .modified()
        .ok()?
        .duration_since(UNIX_EPOCH)
        .ok()?
        .as_millis() as u64;
    let mut seen = Seen::default();
    for line in BufReader::new((&file).take(HEAD))
        .split(b'\n')
        .map_while(Result::ok)
    {
        seen.read(&line);
        if seen.prompt.is_some() && seen.cwd.is_some() {
            break;
        }
    }
    if meta.len() > HEAD {
        let mut tail = Vec::new();
        file.seek(SeekFrom::Start(meta.len() - TAIL)).ok()?;
        file.read_to_end(&mut tail).ok()?;
        // The first line read is likely cut.
        for line in tail.split(|b| *b == b'\n').skip(1) {
            seen.read_title(line);
        }
    } else {
        file.seek(SeekFrom::Start(0)).ok()?;
        for line in BufReader::new(&file).split(b'\n').map_while(Result::ok) {
            seen.read_title(&line);
        }
    }
    let prompt = seen.prompt?;
    Some(Past {
        agent: Agent::Claude,
        id,
        cwd: seen.cwd?,
        title: title_of(seen.title.as_deref().unwrap_or(&prompt)),
        at_ms,
        ..Default::default()
    })
}

#[derive(Default)]
struct Seen {
    cwd: Option<PathBuf>,
    prompt: Option<String>,
    title: Option<String>,
}

impl Seen {
    fn read(&mut self, line: &[u8]) {
        self.read_title(line);
        let Ok(v) = serde_json::from_slice::<Value>(line) else {
            return;
        };
        if self.cwd.is_none()
            && let Some(cwd) = v["cwd"].as_str()
        {
            self.cwd = Some(cwd.into());
        }
        if self.prompt.is_none()
            && v["type"] == "user"
            && v["isMeta"] != true
            && v["isSidechain"] != true
        {
            self.prompt = typed(&v["message"]["content"]);
        }
    }

    /// The name the conversation was last given in `line`, when it gives one.
    fn read_title(&mut self, line: &[u8]) {
        let Ok(v) = serde_json::from_slice::<Value>(line) else {
            return;
        };
        if v["type"] == "custom-title"
            && let Some(title) = v["customTitle"].as_str().filter(|t| !t.trim().is_empty())
        {
            self.title = Some(title.to_string());
        }
    }
}

/// What the user typed in a message's `content`. A command, its output and
/// the notes Claude Code adds come wrapped in tags, and are not.
fn typed(content: &Value) -> Option<String> {
    let text = match content {
        Value::String(text) => text.as_str(),
        Value::Array(parts) => parts
            .iter()
            .find(|p| p["type"] == "text")
            .and_then(|p| p["text"].as_str())?,
        _ => return None,
    };
    let text = text.trim();
    (!text.is_empty() && !text.starts_with('<')).then(|| text.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn line(v: Value) -> String {
        format!("{v}\n")
    }

    fn user(cwd: &str, content: Value) -> String {
        line(json!({"type": "user", "cwd": cwd, "message": {"role": "user", "content": content}}))
    }

    fn write(projects: &Path, dir: &str, id: &str, lines: &[String]) -> PathBuf {
        let folder = projects.join(super::super::places::transcripts_name(Path::new(dir)));
        std::fs::create_dir_all(&folder).unwrap();
        let path = folder.join(format!("{id}.jsonl"));
        std::fs::write(&path, lines.concat()).unwrap();
        path
    }

    #[test]
    fn a_conversation_is_named_by_its_first_prompt_in_the_directory_it_ran_in() {
        let projects = tempfile::tempdir().unwrap();
        write(
            projects.path(),
            "/src/api",
            "a1",
            &[
                line(json!({"type": "permission-mode", "permissionMode": "auto"})),
                user("/src/api", json!("<command-name>/model</command-name>")),
                user("/src/api", json!("fix the\nlogin")),
                user("/src/api", json!("and the logout")),
            ],
        );
        let got = read(projects.path(), Some(Path::new("/src/api")));
        assert_eq!(got.len(), 1);
        assert_eq!(got[0].agent, Agent::Claude);
        assert_eq!(got[0].id, "a1");
        assert_eq!(got[0].cwd, PathBuf::from("/src/api"));
        assert_eq!(got[0].title, "fix the login");
        assert!(got[0].at_ms > 0);
    }

    #[test]
    fn a_conversation_renamed_goes_by_the_name_it_was_last_given() {
        let projects = tempfile::tempdir().unwrap();
        write(
            projects.path(),
            "/r",
            "a1",
            &[
                line(json!({"type": "custom-title", "customTitle": "first"})),
                user("/r", json!([{"type": "text", "text": "hello"}])),
                line(json!({"type": "custom-title", "customTitle": "handoff"})),
            ],
        );
        let got = read(projects.path(), None);
        assert_eq!(got[0].title, "handoff");
    }

    #[test]
    fn the_name_given_late_in_a_long_conversation_is_read_from_its_end() {
        let projects = tempfile::tempdir().unwrap();
        let filler = line(json!({"type": "assistant", "pad": "x".repeat(1024)}));
        let mut lines = vec![user("/r", json!("hello"))];
        lines.extend(std::iter::repeat_n(filler, 400));
        lines.push(line(json!({"type": "custom-title", "customTitle": "late"})));
        write(projects.path(), "/r", "a1", &lines);
        let got = read(projects.path(), None);
        assert_eq!(got[0].title, "late");
    }

    #[test]
    fn a_conversation_no_prompt_was_written_in_is_not_listed() {
        let projects = tempfile::tempdir().unwrap();
        write(
            projects.path(),
            "/r",
            "empty",
            &[line(json!({"type": "permission-mode", "cwd": "/r"}))],
        );
        write(
            projects.path(),
            "/r",
            "meta",
            &[line(json!({"type": "user", "isMeta": true, "cwd": "/r",
                "message": {"content": "Caveat"}}))],
        );
        assert!(read(projects.path(), None).is_empty());
    }

    #[test]
    fn a_directory_s_conversations_leave_out_another_s_that_shares_its_folder() {
        let projects = tempfile::tempdir().unwrap();
        write(projects.path(), "/a_b", "one", &[user("/a_b", json!("hi"))]);
        write(projects.path(), "/a.b", "two", &[user("/a.b", json!("hi"))]);
        let got = read(projects.path(), Some(Path::new("/a_b")));
        let ids: Vec<&str> = got.iter().map(|p| p.id.as_str()).collect();
        assert_eq!(ids, ["one"]);
    }

    #[test]
    fn every_directory_s_conversations_are_listed_without_one_named() {
        let projects = tempfile::tempdir().unwrap();
        write(projects.path(), "/a", "one", &[user("/a", json!("hi"))]);
        write(projects.path(), "/b", "two", &[user("/b", json!("hi"))]);
        let mut ids: Vec<String> = read(projects.path(), None)
            .into_iter()
            .map(|p| p.id)
            .collect();
        ids.sort();
        assert_eq!(ids, ["one", "two"]);
    }

    #[test]
    fn nothing_is_listed_where_claude_code_kept_nothing() {
        let projects = tempfile::tempdir().unwrap();
        assert!(read(&projects.path().join("missing"), None).is_empty());
        assert!(read(projects.path(), Some(Path::new("/nowhere"))).is_empty());
    }
}
