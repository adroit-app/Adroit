//! A question put to Claude Code as a control request, in a process started
//! only to answer it.

use std::path::Path;
use std::time::Duration;

use serde_json::Value;

use crate::Started;

/// The arguments that make Claude Code take control requests on its input and
/// answer on its output, keeping no session of what passes.
pub(crate) const PROBE_ARGS: &[&str] = &[
    "--output-format",
    "stream-json",
    "--verbose",
    "--input-format",
    "stream-json",
    "--no-session-persistence",
];

/// Start Claude Code with `start` in `dir`, write `request` and read its
/// output a line at a time through `read`, until `read` settles it: `Some`
/// with the answer, or with `None` for a refusal. Gives up after `timeout`,
/// and kills the process either way.
pub(crate) async fn ask<T>(
    start: impl FnOnce(Vec<String>) -> Started,
    dir: &Path,
    request: &str,
    timeout: Duration,
    mut read: impl FnMut(&str) -> Option<Option<T>>,
) -> Option<T> {
    let started = start(PROBE_ARGS.iter().map(|a| a.to_string()).collect());
    crate::child::converse(started, dir, timeout, async |talk| {
        talk.write(request).await?;
        talk.until(&mut read).await.flatten()
    })
    .await
}

/// The id every request adroit writes carries, which its answer echoes.
pub(crate) const REQUEST_ID: &str = "adroit";

/// What one line of Claude Code's output says about a request adroit wrote.
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum Answer {
    /// The answer, the `response` it carries.
    Success(Value),
    /// Claude Code answered with an error.
    Refused,
    /// Not the answer: another message, or no JSON at all.
    Other,
}

/// Read one line of Claude Code's output for the answer to adroit's request.
pub(crate) fn answer(line: &str) -> Answer {
    let Ok(msg) = serde_json::from_str::<Value>(line) else {
        return Answer::Other;
    };
    let response = &msg["response"];
    if msg["type"] != "control_response" || response["request_id"] != REQUEST_ID {
        return Answer::Other;
    }
    if response["subtype"] != "success" {
        return Answer::Refused;
    }
    Answer::Success(response["response"].clone())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::{control_error, control_success};

    #[test]
    fn the_process_keeps_no_session() {
        assert!(PROBE_ARGS.contains(&"--no-session-persistence"));
    }

    #[test]
    fn a_success_for_adroits_request_carries_its_response() {
        let line = control_success(r#"{"x":1}"#);
        assert_eq!(answer(&line), Answer::Success(serde_json::json!({"x":1})));
    }

    #[test]
    fn an_error_for_adroits_request_is_a_refusal() {
        let line = control_error("nope");
        assert_eq!(answer(&line), Answer::Refused);
    }

    #[test]
    fn answers_to_other_requests_and_other_messages_are_passed_over() {
        let other = r#"{"type":"control_response","response":{"subtype":"success","request_id":"x","response":{}}}"#;
        assert_eq!(answer(other), Answer::Other);
        assert_eq!(
            answer(r#"{"type":"system","subtype":"init"}"#),
            Answer::Other
        );
        assert_eq!(answer("not json"), Answer::Other);
        assert_eq!(answer("42"), Answer::Other);
    }
}
