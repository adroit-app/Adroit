//! Read an OpenCode conversation, as `opencode session export` hands it
//! over, into the overview's shared link collector, and count what it did.
//! OpenCode keeps it in a database of its own, which every conversation of
//! the login writes to, so it is exported again once the database changes,
//! apart from the reader that asks and no more often than a busy login
//! warrants.

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant, SystemTime};

use adroit_model::{ConversationStats, Tokens};
use serde_json::{Value, json};

use crate::claude::links::{Collector, Links};
use crate::{AgentContext, ConversationReader};

/// The tools whose calls name no link of the conversation's own: what they
/// find is the contents of files and of the shell.
const SEARCHING: [&str; 3] = ["shell", "grep", "glob"];

/// How long an export stands for the conversation when the database cannot
/// tell whether it changed.
const STALE_AFTER: Duration = Duration::from_secs(30);

/// How long an export stands for the conversation while the database keeps
/// being written: some conversation of the login is running a turn.
const BUSY_FOR: Duration = Duration::from_secs(20);

/// How long the database is left unwritten before every conversation of the
/// login counts as settled, and is exported however recently it was.
const SETTLED_AFTER: Duration = Duration::from_secs(3);

/// When the database was last written as of an export, and when that export
/// was asked for.
type Asked = (Vec<Option<SystemTime>>, Instant);

/// An export running apart from the reader: what it was asked for at, and
/// the conversation as it was exported, if it was.
type Running = std::thread::JoinHandle<(Asked, Option<Value>)>;

pub(super) struct Transcript {
    context: AgentContext,
    id: String,
    cwd: PathBuf,
    home: PathBuf,
    /// The folder OpenCode keeps its data in for the login.
    data: PathBuf,
    /// The last export asked for, whether or not OpenCode made it.
    exported: Option<Asked>,
    running: Option<Running>,
    links: Links,
    stats: ConversationStats,
}

impl Transcript {
    /// The reader of the conversation `id`, which `context`'s login keeps,
    /// of a session in `cwd`, `~` standing for `home`.
    pub(super) fn open(context: AgentContext, id: String, cwd: &Path, home: &Path) -> Self {
        let data = match &context.home {
            Some(home) => home.join("data"),
            None => adroit_model::paths::data_dir(),
        };
        Self {
            context,
            data,
            id,
            cwd: cwd.into(),
            home: home.into(),
            exported: None,
            running: None,
            links: Links::default(),
            stats: ConversationStats::default(),
        }
    }

    /// The files OpenCode keeps its database in for `context`'s login.
    fn database(&self) -> [PathBuf; 2] {
        let db = self.data.join("opencode").join("opencode.db");
        [db.with_extension("db-wal"), db]
    }

    /// Takes in the export that ended since last asked, and asks for another
    /// once one is due and none is running. OpenCode takes seconds to export,
    /// so the reader goes on with what it read before rather than wait.
    fn refresh(&mut self) {
        if let Some(done) = self.running.take_if(|running| running.is_finished())
            && let Ok((asked, exported)) = done.join()
        {
            if let Some(exported) = exported {
                self.take(&exported);
            }
            self.exported = Some(asked);
        }
        if self.running.is_some() {
            return;
        }
        let written: Vec<Option<SystemTime>> = self
            .database()
            .iter()
            .map(|file| std::fs::metadata(file).and_then(|m| m.modified()).ok())
            .collect();
        let last = self
            .exported
            .as_ref()
            .map(|(was, at)| (was.as_slice(), at.elapsed()));
        if !due(last, &written, SystemTime::now()) {
            return;
        }
        let args = ["session", "export", &self.id, super::STANDALONE]
            .map(String::from)
            .to_vec();
        let start = crate::launch_in(&self.context, args, Vec::new());
        let cwd = self.cwd.clone();
        self.running = Some(std::thread::spawn(move || {
            let asked = (written, Instant::now());
            let exported = match super::cli::ask_blocking(start, &cwd, super::cli::ASKED_FOR) {
                Some((Some(0), out)) => serde_json::from_str::<Value>(&out).ok(),
                _ => None,
            };
            (asked, exported)
        }));
    }

    /// Everything read from the conversation `exported`, in place of what was
    /// read before.
    fn take(&mut self, exported: &Value) {
        let mut found = Collector::new(&self.cwd, &self.home);
        let mut stats = ConversationStats::default();
        for message in exported["messages"].as_array().into_iter().flatten() {
            let at = message["time"]["created"].as_u64();
            let feed = |found: &mut Collector, role: &str, content: Value| {
                found.feed(
                    &json!({"type":role, "timestamp":at.map(iso), "message":{"content":content}})
                        .to_string(),
                    &|p| p.is_file(),
                );
            };
            match message["type"].as_str() {
                Some("user") => {
                    stats.user_turns += 1;
                    stats.last_user_ms = stats.last_user_ms.max(at);
                    feed(
                        &mut found,
                        "user",
                        json!([{"type":"text", "text":message["text"]}]),
                    );
                }
                Some("assistant") => {
                    count(&mut stats, message);
                    let mut content = Vec::new();
                    let mut results = Vec::new();
                    for part in message["content"].as_array().into_iter().flatten() {
                        match part["type"].as_str() {
                            Some("text") => {
                                content.push(json!({"type":"text", "text":part["text"]}));
                            }
                            Some("tool") => {
                                stats.tool_calls += 1;
                                if let Some((call, result)) = self.call(part) {
                                    content.push(call);
                                    results.extend(result);
                                }
                            }
                            _ => {}
                        }
                    }
                    feed(&mut found, "assistant", json!(content));
                    if !results.is_empty() {
                        feed(&mut found, "user", json!(results));
                    }
                }
                _ => {}
            }
        }
        self.links = found.links();
        self.stats = stats;
    }

    /// The call `part` as the collector reads one, OpenCode's file tools as
    /// the ones it knows by the file's whole path, and its result once it
    /// has one; none for a search.
    fn call(&self, part: &Value) -> Option<(Value, Option<Value>)> {
        let name = part["name"].as_str()?;
        let id = part["id"].as_str()?;
        if SEARCHING.contains(&name) {
            return None;
        }
        let input = &part["state"]["input"];
        let path = input["filePath"].as_str().or(input["path"].as_str());
        let call = match (name, path) {
            ("read" | "edit" | "write", Some(path)) => {
                let name = match name {
                    "read" => "Read",
                    "edit" => "Edit",
                    _ => "Write",
                };
                json!({"type":"tool_use", "id":id, "name":name,
                    "input":{"file_path":self.whole(path)}})
            }
            _ => json!({"type":"tool_use", "id":id, "name":name, "input":input}),
        };
        let state = &part["state"];
        let result = (state["status"] == "completed").then(|| {
            let texts: Vec<Value> = state["content"]
                .as_array()
                .into_iter()
                .flatten()
                .filter(|c| c["type"] == "text")
                .map(|c| json!({"type":"text", "text":c["text"]}))
                .collect();
            json!({"type":"tool_result", "tool_use_id":id, "content":texts})
        });
        Some((call, result))
    }

    fn whole(&self, path: &str) -> String {
        match path.strip_prefix("~/") {
            Some(rest) => self.home.join(rest),
            None => self.cwd.join(path),
        }
        .to_string_lossy()
        .into_owned()
    }
}

/// Whether the conversation is exported again, the database written as
/// `written` at `now`, the `last` export asked for when it was as the files
/// say and how long ago. A database that keeps changing is some conversation
/// of the login at work, maybe another one, so it is exported at most once
/// in [`BUSY_FOR`], and once more as soon as the writing stops.
fn due(
    last: Option<(&[Option<SystemTime>], Duration)>,
    written: &[Option<SystemTime>],
    now: SystemTime,
) -> bool {
    let Some((was, ago)) = last else {
        return true;
    };
    if was == written {
        return written.iter().all(Option::is_none) && ago >= STALE_AFTER;
    }
    let quiet = written
        .iter()
        .flatten()
        .max()
        .and_then(|newest| now.duration_since(*newest).ok());
    ago >= BUSY_FOR || quiet.is_some_and(|quiet| quiet >= SETTLED_AFTER)
}

/// What the assistant's `message` tells of the model and the tokens used.
fn count(stats: &mut ConversationStats, message: &Value) {
    if let Some(model) = message["model"]["id"].as_str().filter(|m| !m.is_empty()) {
        stats.model = Some(model.into());
    }
    let used = &message["tokens"];
    if used.is_object() {
        let n = |value: &Value| value.as_u64().unwrap_or(0);
        let tokens = stats.tokens.get_or_insert_with(Tokens::default);
        tokens.input += n(&used["input"]) + n(&used["cache"]["read"]) + n(&used["cache"]["write"]);
        tokens.output += n(&used["output"]);
        tokens.cached += n(&used["cache"]["read"]);
    }
}

/// `ms`, milliseconds since the epoch, as the time the collector reads.
fn iso(ms: u64) -> String {
    let (days, rest) = ((ms / 86_400_000) as i64, ms % 86_400_000);
    let (year, month, day) = crate::time::civil_from_days(days);
    format!(
        "{year:04}-{month:02}-{day:02}T{:02}:{:02}:{:02}.{:03}Z",
        rest / 3_600_000,
        rest / 60_000 % 60,
        rest / 1000 % 60,
        rest % 1000
    )
}

impl ConversationReader for Transcript {
    fn stats(&self) -> ConversationStats {
        self.stats.clone()
    }

    fn read(&mut self) -> Links {
        self.refresh();
        self.links.clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::claude::links::{Kind, Touch};
    use adroit_model::Agent;

    /// A reader of `exported`, as an `opencode` standing in for OpenCode's
    /// hands it over, of a session in `cwd`; the stand-in counts the times
    /// it is asked in `asked`.
    fn reader(cwd: &Path, exported: &Value) -> (Transcript, tempfile::TempDir, PathBuf) {
        let (dir, opencode) = crate::testing::program(
            "opencode",
            &format!(
                "echo x >> \"$0.asked\"\n[ \"$*\" = 'session export ses_a --standalone' ] || exit 1\ncat <<'EOF'\n{exported}\nEOF"
            ),
        );
        let mut context = AgentContext::builtin(Agent::OpenCode);
        context.launch.command = vec![opencode.display().to_string()];
        let asked = opencode.with_extension("asked");
        let mut reader = Transcript::open(context, "ses_a".into(), cwd, cwd);
        reader.data = dir.path().join("data");
        (reader, dir, asked)
    }

    fn asked(path: &Path) -> usize {
        std::fs::read_to_string(path).map_or(0, |s| s.lines().count())
    }

    /// What `reader` reads once the export it asks for has ended.
    fn settled(reader: &mut Transcript) -> Links {
        let mut links = reader.read();
        for _ in 0..3000 {
            if reader.running.is_none() {
                return links;
            }
            std::thread::sleep(Duration::from_millis(10));
            links = reader.read();
        }
        panic!("the export did not end");
    }

    #[test]
    fn prompts_replies_and_file_tools_supply_links_and_the_conversation_is_counted() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("src.rs"), "fn main() {}").unwrap();
        std::fs::write(dir.path().join("report.md"), "report").unwrap();
        let exported = json!({"info":{"id":"ses_a"}, "messages":[
            {"type":"user", "id":"m1", "time":{"created":1_792_859_012_076_u64}, "text":"see https://user.example"},
            {"type":"assistant", "id":"m2", "time":{"created":1_792_859_012_088_u64},
                "model":{"id":"slow", "providerID":"fake"},
                "tokens":{"input":10, "output":6, "reasoning":0, "cache":{"read":4, "write":0}},
                "content":[
                    {"type":"reasoning", "text":"https://thought.example"},
                    {"type":"text", "text":"[Report](report.md:3) https://docs.example"},
                    {"type":"tool", "id":"t1", "name":"read",
                        "state":{"status":"completed", "input":{"filePath":"src.rs"},
                            "content":[{"type":"text", "text":"https://inside-file.example"}]}},
                    {"type":"tool", "id":"t2", "name":"shell",
                        "state":{"status":"completed", "input":{"command":"curl https://shell.example"},
                            "content":[{"type":"text", "text":"https://shell-output.example"}]}}]},
            {"type":"idle", "id":"m3", "time":{"created":1_792_859_013_981_u64}, "outcome":"succeeded"}
        ]});
        let (mut reader, _held, _) = reader(dir.path(), &exported);
        let links = settled(&mut reader);
        let urls: Vec<&str> = links.urls.iter().map(|l| l.target.as_str()).collect();
        assert_eq!(urls, ["https://docs.example", "https://user.example"]);
        let touched: Vec<(String, Kind)> = links
            .files
            .iter()
            .map(|l| (l.target.clone(), l.kind))
            .collect();
        assert!(touched.contains(&(
            dir.path().join("src.rs").display().to_string(),
            Kind::File(Touch::Read)
        )));
        let stats = reader.stats();
        assert_eq!(stats.model.as_deref(), Some("slow"));
        assert_eq!(stats.user_turns, 1);
        assert_eq!(stats.tool_calls, 2);
        assert_eq!(stats.last_user_ms, Some(1_792_859_012_076));
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
    fn the_conversation_is_exported_apart_from_the_reader_and_again_only_once_the_database_changes()
    {
        let dir = tempfile::tempdir().unwrap();
        let exported = json!({"messages":[{"type":"user", "text":"hi", "time":{"created":1}}]});
        let (mut reader, _held, count) = reader(dir.path(), &exported);
        let db = reader.database()[1].clone();
        std::fs::create_dir_all(db.parent().unwrap()).unwrap();
        std::fs::write(&db, "").unwrap();
        let earlier = SystemTime::now() - SETTLED_AFTER;
        let modified = |at: SystemTime| {
            std::fs::File::options()
                .write(true)
                .open(&db)
                .unwrap()
                .set_modified(at)
                .unwrap()
        };
        modified(earlier);
        // The first read goes on without the export it asked for.
        assert_eq!(reader.read().urls, Vec::new());
        settled(&mut reader);
        settled(&mut reader);
        assert_eq!(asked(&count), 1);
        assert_eq!(reader.stats().user_turns, 1);
        modified(earlier - Duration::from_secs(1));
        settled(&mut reader);
        assert_eq!(asked(&count), 2);
    }

    #[test]
    fn a_busy_database_is_exported_at_most_once_in_a_while_and_again_once_it_settles() {
        let at = SystemTime::UNIX_EPOCH + Duration::from_secs(1_000_000);
        let then = [Some(at), Some(at)];
        let written = [Some(at + Duration::from_secs(5)), Some(at)];
        let unknown = [None, None];
        let just = Duration::from_secs(1);
        let busy = at + Duration::from_secs(5);
        let settled = busy + SETTLED_AFTER;
        assert!(due(None, &written, busy));
        // Unchanged, it is not asked again, unless the files cannot say.
        assert!(!due(Some((&then, BUSY_FOR * 10)), &then, settled));
        assert!(!due(Some((&unknown, just)), &unknown, settled));
        assert!(due(Some((&unknown, STALE_AFTER)), &unknown, settled));
        // Changed while written to, it waits out the while.
        assert!(!due(Some((&then, just)), &written, busy));
        assert!(due(Some((&then, BUSY_FOR)), &written, busy));
        // Changed and left alone, it is asked at once.
        assert!(due(Some((&then, just)), &written, settled));
    }

    #[test]
    fn times_are_written_as_the_collector_reads_them() {
        assert_eq!(iso(0), "1970-01-01T00:00:00.000Z");
        assert_eq!(iso(1_792_859_012_076), "2026-10-24T16:23:32.076Z");
        assert_eq!(
            crate::time::timestamp_ms(&json!(iso(1_792_859_012_076))),
            Some(1_792_859_012_076)
        );
    }
}
