//! The slash commands Claude Code offers in a directory, asked of Claude Code
//! itself: its built-in commands and the account's skills, plugins and MCP
//! prompts, as the release installed knows them.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use serde_json::Value;

use super::control;
use crate::Started;

pub use adroit_model::Command;

/// What one line Claude Code printed says about the commands asked for.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Reply {
    /// The commands.
    Commands(Vec<Command>),
    /// Claude Code answered, but not with commands.
    Refused,
    /// Not the answer: another message, or no JSON at all.
    Other,
}

/// The one line written to Claude Code: an `initialize` control request,
/// never a user message, so no turn runs and no tokens are spent.
pub(crate) const REQUEST: &str = "{\"request_id\":\"adroit\",\"type\":\"control_request\",\"request\":{\"subtype\":\"initialize\"}}\n";

/// Read one line of Claude Code's output.
pub fn reply(line: &str) -> Reply {
    let answer = match control::answer(line) {
        control::Answer::Success(v) => v,
        control::Answer::Refused => return Reply::Refused,
        control::Answer::Other => return Reply::Other,
    };
    let Some(list) = answer["commands"].as_array() else {
        return Reply::Refused;
    };
    let text = |v: &Value| v.as_str().unwrap_or_default().to_string();
    Reply::Commands(
        list.iter()
            .filter(|c| c["name"].as_str().is_some_and(|n| !n.is_empty()))
            .map(|c| Command {
                name: text(&c["name"]),
                description: text(&c["description"]),
                argument_hint: text(&c["argumentHint"]),
            })
            .collect(),
    )
}

/// How long Claude Code is given to answer.
pub const PROBE_TIMEOUT: Duration = Duration::from_secs(5);

/// Ask Claude Code, started by `start`, which commands it offers in `dir`, giving up
/// after `timeout`. The process is killed once it answers. `None` for every
/// failure: the request is unversioned, and a release that changes it leaves
/// `/` with nothing to offer rather than with a list of guesses.
pub async fn probe(
    start: impl FnOnce(Vec<String>) -> Started,
    dir: &Path,
    timeout: Duration,
) -> Option<Vec<Command>> {
    control::ask(start, dir, REQUEST, timeout, |line| match reply(line) {
        Reply::Commands(list) => Some(Some(list)),
        Reply::Refused => Some(None),
        Reply::Other => None,
    })
    .await
}

/// How long a directory's commands are offered before they are asked for
/// again. Short, as skills, plugins and MCP servers come and go while
/// sessions run, and asking costs a process for a moment and no tokens.
pub const COMMANDS_TTL: Duration = Duration::from_secs(60);

#[derive(Debug, Default)]
struct Entry {
    commands: Vec<Command>,
    asked: Option<Instant>,
    asking: bool,
}

/// Each directory's commands, with when they were asked for: a stale list is
/// offered until a fresh one arrives, and one directory is asked once at a
/// time.
#[derive(Debug, Default)]
pub struct Cache {
    dirs: HashMap<PathBuf, Entry>,
}

impl Cache {
    /// The commands known for `dir`, however old; none before the first
    /// answer.
    pub fn get(&self, dir: &Path) -> &[Command] {
        self.dirs.get(dir).map_or(&[], |e| &e.commands)
    }

    /// Whether `dir` should be asked now, at `now`: never asked, or asked
    /// longer than [`COMMANDS_TTL`] ago, and not being asked. A yes counts as
    /// asking until [`Cache::settle`].
    pub fn want(&mut self, dir: &Path, now: Instant) -> bool {
        let e = self.dirs.entry(dir.to_path_buf()).or_default();
        let stale = e
            .asked
            .is_none_or(|at| now.saturating_duration_since(at) >= COMMANDS_TTL);
        if e.asking || !stale {
            return false;
        }
        e.asking = true;
        true
    }

    /// Take the answer for `dir`, at `now`. A failure keeps the list there
    /// was, and waits the same time as an answer before asking again.
    pub fn settle(&mut self, dir: &Path, now: Instant, got: Option<Vec<Command>>) {
        let e = self.dirs.entry(dir.to_path_buf()).or_default();
        e.asking = false;
        e.asked = Some(now);
        if let Some(list) = got {
            e.commands = list;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const ANSWER: &str = r#"{"type":"control_response","response":{"subtype":"success","request_id":"adroit","response":{"commands":[{"name":"review","description":"Review a pull request","argumentHint":"[pr]"},{"name":"gh-as:account","description":"(gh-as) Run gh as the account","argumentHint":"","aliases":["account"]},{"name":""},{"description":"no name"}],"models":[]}}}"#;

    fn command(name: &str, description: &str, argument_hint: &str) -> Command {
        Command {
            name: name.into(),
            description: description.into(),
            argument_hint: argument_hint.into(),
        }
    }

    #[test]
    fn the_answer_to_initialize_lists_the_commands_with_their_hints() {
        assert_eq!(
            reply(ANSWER),
            Reply::Commands(vec![
                command("review", "Review a pull request", "[pr]"),
                command("gh-as:account", "(gh-as) Run gh as the account", ""),
            ])
        );
    }

    #[test]
    fn an_answer_for_another_request_is_not_the_answer() {
        let other = ANSWER.replace(r#""request_id":"adroit""#, r#""request_id":"x""#);
        assert_eq!(reply(&other), Reply::Other);
    }

    #[test]
    fn an_error_answer_is_a_refusal() {
        let line = r#"{"type":"control_response","response":{"subtype":"error","request_id":"adroit","error":"nope"}}"#;
        assert_eq!(reply(line), Reply::Refused);
    }

    #[test]
    fn a_success_without_commands_is_a_refusal() {
        let line = r#"{"type":"control_response","response":{"subtype":"success","request_id":"adroit","response":{}}}"#;
        assert_eq!(reply(line), Reply::Refused);
    }

    #[test]
    fn other_messages_and_garbage_are_passed_over() {
        assert_eq!(reply(r#"{"type":"system","subtype":"init"}"#), Reply::Other);
        assert_eq!(reply("not json"), Reply::Other);
        assert_eq!(reply(""), Reply::Other);
    }

    #[test]
    fn the_request_asks_only_to_initialize() {
        let sent: serde_json::Value = serde_json::from_str(REQUEST.trim_end()).unwrap();
        assert_eq!(sent["type"], "control_request");
        assert_eq!(sent["request"]["subtype"], "initialize");
        assert_eq!(sent["request_id"], control::REQUEST_ID);
        assert!(REQUEST.ends_with('\n'));
    }

    fn dir() -> PathBuf {
        PathBuf::from("/src/api")
    }

    #[test]
    fn a_directory_never_asked_is_asked_once_until_it_answers() {
        let mut cache = Cache::default();
        let t = Instant::now();
        assert!(cache.want(&dir(), t));
        assert!(!cache.want(&dir(), t + COMMANDS_TTL * 2));
        assert!(cache.get(&dir()).is_empty());
    }

    #[test]
    fn a_fresh_answer_is_offered_without_asking_again() {
        let mut cache = Cache::default();
        let t = Instant::now();
        cache.want(&dir(), t);
        cache.settle(&dir(), t, Some(vec![command("review", "", "")]));
        assert!(!cache.want(&dir(), t + COMMANDS_TTL - Duration::from_secs(1)));
        assert_eq!(cache.get(&dir()), [command("review", "", "")]);
    }

    #[test]
    fn a_stale_answer_is_asked_for_again_once_and_offered_meanwhile() {
        let mut cache = Cache::default();
        let t = Instant::now();
        cache.want(&dir(), t);
        cache.settle(&dir(), t, Some(vec![command("review", "", "")]));
        let later = t + COMMANDS_TTL;
        assert!(cache.want(&dir(), later));
        assert!(!cache.want(&dir(), later));
        assert_eq!(cache.get(&dir()), [command("review", "", "")]);
        cache.settle(&dir(), later, Some(vec![command("plan", "", "")]));
        assert_eq!(cache.get(&dir()), [command("plan", "", "")]);
    }

    #[test]
    fn a_failure_keeps_the_list_there_was_and_waits_before_asking_again() {
        let mut cache = Cache::default();
        let t = Instant::now();
        cache.want(&dir(), t);
        cache.settle(&dir(), t, Some(vec![command("review", "", "")]));
        let later = t + COMMANDS_TTL;
        cache.want(&dir(), later);
        cache.settle(&dir(), later, None);
        assert_eq!(cache.get(&dir()), [command("review", "", "")]);
        assert!(!cache.want(&dir(), later + Duration::from_secs(1)));
        assert!(cache.want(&dir(), later + COMMANDS_TTL));
    }

    #[test]
    fn each_directory_is_asked_on_its_own() {
        let mut cache = Cache::default();
        let t = Instant::now();
        assert!(cache.want(&dir(), t));
        assert!(cache.want(Path::new("/src/web"), t));
    }

    /// A stand-in for Claude Code running `body`, a shell script, in a
    /// directory of its own; the program's path.
    fn fake(name: &str, body: &str) -> PathBuf {
        use std::os::unix::fs::PermissionsExt;
        let dir =
            std::env::temp_dir().join(format!("adroit-commands-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let program = dir.join("claude");
        std::fs::write(&program, format!("#!/bin/sh\n{body}\n")).unwrap();
        std::fs::set_permissions(&program, std::fs::Permissions::from_mode(0o755)).unwrap();
        program
    }

    async fn ask(program: &Path, timeout: Duration) -> Option<Vec<Command>> {
        let dir = program.parent().unwrap();
        let program = program.to_str().unwrap().to_string();
        probe(|args| (program, args, Vec::new()), dir, timeout).await
    }

    #[tokio::test]
    async fn the_probe_starts_claude_code_as_it_is_set_up_to_start() {
        let p = fake(
            "wrapped",
            &format!(
                "[ \"$1\" = --wrapped ] && [ \"$2\" = --output-format ] && [ \"$WRAPPED\" = yes ] || exit 1\nread line\necho '{ANSWER}'\nexec sleep 30"
            ),
        );
        let program = p.to_str().unwrap().to_string();
        let start = |args: Vec<String>| {
            let args = std::iter::once("--wrapped".to_string())
                .chain(args)
                .collect();
            (
                program,
                args,
                vec![("WRAPPED".to_string(), "yes".to_string())],
            )
        };

        let got = probe(start, p.parent().unwrap(), Duration::from_secs(5)).await;

        assert_eq!(got.map(|c| c.len()), Some(2));
    }

    #[tokio::test]
    async fn the_probe_reads_past_other_messages_to_the_answer() {
        let p = fake(
            "answer",
            &format!("read line\necho '{{\"type\":\"system\"}}'\necho '{ANSWER}'\nexec sleep 30"),
        );
        let got = ask(&p, Duration::from_secs(5)).await.unwrap();
        assert_eq!(got.len(), 2);
    }

    #[tokio::test]
    async fn the_probe_writes_nothing_but_the_initialize_request() {
        let p = fake(
            "written",
            &format!(
                "IFS= read -r line\nprintf '%s\\n' \"$line\" > \"$(dirname \"$0\")/stdin\"\necho '{ANSWER}'\nexec sleep 30"
            ),
        );
        ask(&p, Duration::from_secs(5)).await.unwrap();
        let written = p.parent().unwrap().join("stdin");
        assert_eq!(std::fs::read_to_string(&written).unwrap(), REQUEST);
    }

    #[tokio::test]
    async fn a_program_that_does_not_answer_in_time_offers_nothing() {
        let p = fake("silent", "exec sleep 30");
        let started = Instant::now();
        assert_eq!(ask(&p, Duration::from_millis(300)).await, None);
        assert!(started.elapsed() < Duration::from_secs(5));
    }

    #[tokio::test]
    async fn a_program_that_exits_without_answering_offers_nothing() {
        let p = fake("exits", "read line\necho oops");
        assert_eq!(ask(&p, Duration::from_secs(5)).await, None);
    }

    #[tokio::test]
    async fn a_missing_program_offers_nothing() {
        let got = probe(
            |args| ("/nonexistent/claude".to_string(), args, Vec::new()),
            Path::new("/"),
            Duration::from_secs(1),
        )
        .await;
        assert_eq!(got, None);
    }

    /// Needs a `claude` on PATH that is logged in, which CI has not.
    #[tokio::test]
    #[ignore = "needs Claude Code installed and logged in"]
    async fn claude_code_answers_with_its_built_in_commands() {
        let start = |args| ("claude".to_string(), args, Vec::new());
        let got = probe(start, &std::env::temp_dir(), PROBE_TIMEOUT)
            .await
            .expect("an answer");
        assert!(got.iter().any(|c| c.name == "compact"), "{got:?}");
    }
}
