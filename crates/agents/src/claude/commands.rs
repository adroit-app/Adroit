//! The slash commands Claude Code offers in a directory, asked of Claude Code
//! itself: its built-in commands and the account's skills, plugins and MCP
//! prompts, as the release installed knows them.

use std::path::Path;
use std::time::Duration;

use serde_json::Value;

use super::control;
use crate::Started;

pub use crate::commands::*;

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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::{control_error, control_success, launching, program};
    use std::time::Instant;

    fn answer() -> String {
        control_success(
            r#"{"commands":[{"name":"review","description":"Review a pull request","argumentHint":"[pr]"},{"name":"gh-as:account","description":"(gh-as) Run gh as the account","argumentHint":"","aliases":["account"]},{"name":""},{"description":"no name"}],"models":[]}"#,
        )
    }

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
            reply(&answer()),
            Reply::Commands(vec![
                command("review", "Review a pull request", "[pr]"),
                command("gh-as:account", "(gh-as) Run gh as the account", ""),
            ])
        );
    }

    #[test]
    fn an_answer_for_another_request_is_not_the_answer() {
        let other = answer().replace(r#""request_id":"adroit""#, r#""request_id":"x""#);
        assert_eq!(reply(&other), Reply::Other);
    }

    #[test]
    fn an_error_answer_is_a_refusal() {
        let line = control_error("nope");
        assert_eq!(reply(&line), Reply::Refused);
    }

    #[test]
    fn a_success_without_commands_is_a_refusal() {
        let line = control_success("{}");
        assert_eq!(reply(&line), Reply::Refused);
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

    async fn ask(program: &Path, timeout: Duration) -> Option<Vec<Command>> {
        probe(launching(program), program.parent().unwrap(), timeout).await
    }

    #[tokio::test]
    async fn the_probe_starts_claude_code_as_it_is_set_up_to_start() {
        let answer = answer();
        let (_dir, p) = program(
            "claude",
            &format!(
                "[ \"$1\" = --wrapped ] && [ \"$2\" = --output-format ] && [ \"$WRAPPED\" = yes ] || exit 1\nread line\necho '{answer}'\nexec sleep 30"
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
        let answer = answer();
        let (_dir, p) = program(
            "claude",
            &format!("read line\necho '{{\"type\":\"system\"}}'\necho '{answer}'\nexec sleep 30"),
        );
        let got = ask(&p, Duration::from_secs(5)).await.unwrap();
        assert_eq!(got.len(), 2);
    }

    #[tokio::test]
    async fn the_probe_writes_nothing_but_the_initialize_request() {
        let answer = answer();
        let (_dir, p) = program(
            "claude",
            &format!(
                "IFS= read -r line\nprintf '%s\\n' \"$line\" > \"$(dirname \"$0\")/stdin\"\necho '{answer}'\nexec sleep 30"
            ),
        );
        ask(&p, Duration::from_secs(5)).await.unwrap();
        let written = p.parent().unwrap().join("stdin");
        assert_eq!(std::fs::read_to_string(&written).unwrap(), REQUEST);
    }

    #[tokio::test]
    async fn a_program_that_does_not_answer_in_time_offers_nothing() {
        let (_dir, p) = program("claude", "exec sleep 30");
        let started = Instant::now();
        assert_eq!(ask(&p, Duration::from_millis(300)).await, None);
        assert!(started.elapsed() < Duration::from_secs(5));
    }

    #[tokio::test]
    async fn a_program_that_exits_without_answering_offers_nothing() {
        let (_dir, p) = program("claude", "read line\necho oops");
        assert_eq!(ask(&p, Duration::from_secs(5)).await, None);
    }

    #[tokio::test]
    async fn a_missing_program_offers_nothing() {
        let got = probe(
            launching(Path::new("/nonexistent/claude")),
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
        let got = probe(
            launching(Path::new("claude")),
            &std::env::temp_dir(),
            PROBE_TIMEOUT,
        )
        .await
        .expect("an answer");
        assert!(got.iter().any(|c| c.name == "compact"), "{got:?}");
    }
}
