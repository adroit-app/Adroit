//! The models Claude Code accepts, asked of Claude Code itself with the same
//! request as its commands.

use std::path::Path;
use std::time::Duration;

use adroit_model::ProviderModel;
use serde_json::Value;

use crate::{Started, commands, control};

/// Read one line of Claude Code's output: `Some` once it is the answer, with
/// `None` inside when the answer lists no models.
pub fn reply(line: &str) -> Option<Option<Vec<ProviderModel>>> {
    let answer = match control::answer(line) {
        control::Answer::Success(v) => v,
        control::Answer::Refused => return Some(None),
        control::Answer::Other => return None,
    };
    let Some(list) = answer["models"].as_array() else {
        return Some(None);
    };
    let text = |v: &Value| v.as_str().unwrap_or_default().to_string();
    Some(Some(
        list.iter()
            .filter(|m| m["value"].as_str().is_some_and(|v| !v.is_empty()))
            .map(|m| {
                let id = text(&m["value"]);
                ProviderModel {
                    resolved: m["resolvedModel"]
                        .as_str()
                        .filter(|r| !r.is_empty() && *r != id)
                        .map(Into::into),
                    name: m["displayName"].as_str().unwrap_or(&id).to_string(),
                    description: text(&m["description"]),
                    id,
                }
            })
            .collect(),
    ))
}

/// Ask Claude Code, started by `start`, which models it accepts, giving up
/// after `timeout`; `None` for every failure.
pub async fn probe(
    start: impl FnOnce(Vec<String>) -> Started,
    dir: &Path,
    timeout: Duration,
) -> Option<Vec<ProviderModel>> {
    control::ask(start, dir, commands::REQUEST, timeout, reply).await
}

#[cfg(test)]
mod tests {
    use super::*;

    const ANSWER: &str = r#"{"type":"control_response","response":{"subtype":"success","request_id":"adroit","response":{"commands":[],"models":[{"value":"default","resolvedModel":"claude-opus-5-5","displayName":"Default (recommended)","description":"Opus 5.5"},{"value":"claude-sonnet-4-6","resolvedModel":"claude-sonnet-4-6","displayName":"Sonnet 4.6","description":"Efficient"},{"value":""},{"displayName":"no value"}]}}}"#;

    #[test]
    fn the_answer_to_initialize_lists_the_models_with_what_they_resolve_to() {
        assert_eq!(
            reply(ANSWER),
            Some(Some(vec![
                ProviderModel {
                    id: "default".into(),
                    resolved: Some("claude-opus-5-5".into()),
                    name: "Default (recommended)".into(),
                    description: "Opus 5.5".into(),
                },
                ProviderModel {
                    id: "claude-sonnet-4-6".into(),
                    resolved: None,
                    name: "Sonnet 4.6".into(),
                    description: "Efficient".into(),
                },
            ]))
        );
    }

    #[test]
    fn an_answer_without_models_or_an_error_lists_none() {
        let without = r#"{"type":"control_response","response":{"subtype":"success","request_id":"adroit","response":{"commands":[]}}}"#;
        assert_eq!(reply(without), Some(None));
        let error = r#"{"type":"control_response","response":{"subtype":"error","request_id":"adroit","error":"nope"}}"#;
        assert_eq!(reply(error), Some(None));
    }

    #[test]
    fn other_messages_are_passed_over() {
        assert_eq!(reply(r#"{"type":"system","subtype":"init"}"#), None);
        assert_eq!(reply("not json"), None);
    }

    #[tokio::test]
    async fn the_probe_reads_the_models_from_claude_code() {
        use std::os::unix::fs::PermissionsExt;
        let dir = tempfile::tempdir().unwrap();
        let program = dir.path().join("claude");
        std::fs::write(
            &program,
            format!("#!/bin/sh\nread line\necho '{ANSWER}'\nexec sleep 30\n"),
        )
        .unwrap();
        std::fs::set_permissions(&program, std::fs::Permissions::from_mode(0o755)).unwrap();
        let program = program.to_str().unwrap().to_string();

        let got = probe(
            |args| (program, args, Vec::new()),
            dir.path(),
            Duration::from_secs(5),
        )
        .await;

        assert_eq!(got.map(|m| m.len()), Some(2));
    }

    /// Needs a `claude` on PATH that is logged in, which CI has not.
    #[tokio::test]
    #[ignore = "needs Claude Code installed and logged in"]
    async fn claude_code_answers_with_the_models_it_accepts() {
        let start = |args| ("claude".to_string(), args, Vec::new());
        let got = probe(start, &std::env::temp_dir(), commands::PROBE_TIMEOUT)
            .await
            .expect("an answer");
        assert!(got.iter().any(|m| m.id == "opus"), "{got:?}");
    }

    #[tokio::test]
    async fn a_missing_program_lists_nothing() {
        let got = probe(
            |args| ("/nonexistent/claude".to_string(), args, Vec::new()),
            Path::new("/"),
            Duration::from_secs(1),
        )
        .await;
        assert_eq!(got, None);
    }
}
