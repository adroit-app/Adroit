//! How full a conversation's context window is, asked of Claude Code itself:
//! the numbers its `/context` command shows. Transcripts record what each
//! request used but not the window the model has, so it cannot be read from
//! them.

use std::path::Path;
use std::time::{Duration, Instant};

use super::control;
use crate::Started;

/// What one line Claude Code printed says about the context asked for.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Reply {
    /// The tokens the context holds, and the most it can hold.
    Window { used: u64, total: u64 },
    /// Claude Code answered, but not with the context.
    Refused,
    /// Not the answer: another message, or no JSON at all.
    Other,
}

/// The one line written to Claude Code: a `get_context_usage` control
/// request, never a user message, so no turn runs and no tokens are spent.
const REQUEST: &str = "{\"request_id\":\"adroit\",\"type\":\"control_request\",\"request\":{\"subtype\":\"get_context_usage\",\"detail\":\"summary\"}}\n";

/// Read one line of Claude Code's output.
pub fn reply(line: &str) -> Reply {
    let answer = match control::answer(line) {
        control::Answer::Success(v) => v,
        control::Answer::Refused => return Reply::Refused,
        control::Answer::Other => return Reply::Other,
    };
    match (
        answer["totalTokens"].as_u64(),
        answer["maxTokens"].as_u64().filter(|n| *n > 0),
    ) {
        (Some(used), Some(total)) => Reply::Window { used, total },
        _ => Reply::Refused,
    }
}

/// The arguments, past the probe's own, that load the conversation
/// `session_id` as it ran on `model`.
fn resume_args(session_id: &str, model: Option<&str>) -> Vec<String> {
    let mut args = vec!["--resume".to_string(), session_id.to_string()];
    if let Some(model) = model {
        args.extend(["--model".to_string(), model.to_string()]);
    }
    args
}

/// How long Claude Code is given to load the conversation and answer.
pub const PROBE_TIMEOUT: Duration = Duration::from_secs(15);

/// Ask Claude Code, started by `start` in `dir`, how full the context of the
/// conversation `session_id` is, giving up after `timeout`. `None` for every
/// failure: the request is unversioned, and a release that changes it leaves
/// the context unknown rather than guessed.
pub async fn probe(
    start: impl FnOnce(Vec<String>) -> Started,
    dir: &Path,
    session_id: &str,
    model: Option<&str>,
    timeout: Duration,
) -> Option<(u64, u64)> {
    let extra = resume_args(session_id, model);
    let start = move |mut args: Vec<String>| {
        args.extend(extra);
        start(args)
    };
    control::ask(start, dir, REQUEST, timeout, |line| match reply(line) {
        Reply::Window { used, total } => Some(Some((used, total))),
        Reply::Refused => Some(None),
        Reply::Other => None,
    })
    .await
}

/// How long after one answer the context is asked for again, however the
/// conversation has moved: asking starts a process that loads the whole
/// conversation.
pub const CONTEXT_GAP: Duration = Duration::from_secs(30);

/// The context last heard for one conversation, with when it was asked for
/// and how far the conversation had got then.
#[derive(Debug)]
pub struct Fresh<K> {
    key: Option<K>,
    window: Option<(u64, u64)>,
    asked: Option<(Instant, u64)>,
    asking: bool,
}

impl<K> Default for Fresh<K> {
    fn default() -> Self {
        Fresh {
            key: None,
            window: None,
            asked: None,
            asking: false,
        }
    }
}

impl<K: PartialEq + Clone> Fresh<K> {
    /// The context last heard for `key`; `None` before its first answer.
    pub fn get(&self, key: &K) -> Option<(u64, u64)> {
        self.window.filter(|_| self.key.as_ref() == Some(key))
    }

    /// Whether to ask now, at `now`, for the conversation `key` that has got
    /// as far as `mark`. A yes counts as asking until [`Fresh::settle`].
    pub fn want(&mut self, key: &K, mark: u64, now: Instant) -> bool {
        if self.key.as_ref() != Some(key) {
            *self = Fresh {
                key: Some(key.clone()),
                ..Fresh::default()
            };
        }
        let due = self.asked.is_none_or(|(at, was)| {
            was != mark && now.saturating_duration_since(at) >= CONTEXT_GAP
        });
        if self.asking || !due {
            return false;
        }
        self.asking = true;
        self.asked = Some((now, mark));
        true
    }

    /// Take the answer for `key`, at `now`. A failure keeps the context
    /// there was.
    pub fn settle(&mut self, key: &K, now: Instant, got: Option<(u64, u64)>) {
        if self.key.as_ref() != Some(key) {
            return;
        }
        self.asking = false;
        if let Some((at, _)) = &mut self.asked {
            *at = now;
        }
        if got.is_some() {
            self.window = got;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const ANSWER: &str = r#"{"type":"control_response","response":{"subtype":"success","request_id":"adroit","response":{"categories":[],"totalTokens":218868,"maxTokens":1000000,"rawMaxTokens":1000000,"percentage":22}}}"#;

    #[test]
    fn the_request_asks_for_the_context_without_its_breakdown() {
        let v: serde_json::Value = serde_json::from_str(REQUEST).unwrap();
        assert_eq!(v["type"], "control_request");
        assert_eq!(v["request_id"], control::REQUEST_ID);
        assert_eq!(v["request"]["subtype"], "get_context_usage");
        assert_eq!(v["request"]["detail"], "summary");
        assert!(REQUEST.ends_with('\n'));
    }

    #[test]
    fn the_answer_is_the_tokens_held_and_the_window() {
        assert_eq!(
            reply(ANSWER),
            Reply::Window {
                used: 218_868,
                total: 1_000_000
            }
        );
    }

    #[test]
    fn an_answer_without_a_window_is_a_refusal() {
        let line = r#"{"type":"control_response","response":{"subtype":"success","request_id":"adroit","response":{"totalTokens":5,"maxTokens":0}}}"#;
        assert_eq!(reply(line), Reply::Refused);
        let line = r#"{"type":"control_response","response":{"subtype":"error","request_id":"adroit","error":"get_context_usage is not supported"}}"#;
        assert_eq!(reply(line), Reply::Refused);
    }

    #[test]
    fn other_lines_are_not_the_answer() {
        assert_eq!(reply(r#"{"type":"system","subtype":"init"}"#), Reply::Other);
        assert_eq!(reply("not json"), Reply::Other);
    }

    #[test]
    fn the_conversation_is_resumed_on_the_model_it_ran_on() {
        assert_eq!(
            resume_args("abc", Some("claude-opus-5-5")),
            ["--resume", "abc", "--model", "claude-opus-5-5"]
        );
        assert_eq!(resume_args("abc", None), ["--resume", "abc"]);
    }

    #[test]
    fn a_conversation_is_asked_for_at_once_and_once_at_a_time() {
        let now = Instant::now();
        let mut fresh = Fresh::default();
        assert!(fresh.want(&"a", 10, now));
        assert!(!fresh.want(&"a", 20, now + CONTEXT_GAP));
        fresh.settle(&"a", now, Some((5, 100)));
        assert_eq!(fresh.get(&"a"), Some((5, 100)));
    }

    #[test]
    fn it_is_asked_again_only_once_it_moved_and_the_gap_passed() {
        let now = Instant::now();
        let mut fresh = Fresh::default();
        assert!(fresh.want(&"a", 10, now));
        fresh.settle(&"a", now, Some((5, 100)));
        assert!(!fresh.want(&"a", 10, now + CONTEXT_GAP * 2), "not moved");
        assert!(!fresh.want(&"a", 20, now + CONTEXT_GAP / 2), "too soon");
        assert!(fresh.want(&"a", 20, now + CONTEXT_GAP));
    }

    #[test]
    fn another_conversation_is_asked_for_at_once_and_drops_the_old_answer() {
        let now = Instant::now();
        let mut fresh = Fresh::default();
        assert!(fresh.want(&"a", 10, now));
        fresh.settle(&"a", now, Some((5, 100)));
        assert!(fresh.want(&"b", 10, now));
        assert_eq!(fresh.get(&"a"), None);
        assert_eq!(fresh.get(&"b"), None);
        fresh.settle(&"a", now, Some((7, 100)));
        assert_eq!(fresh.get(&"b"), None, "a late answer for a is dropped");
        assert!(!fresh.want(&"b", 10, now), "b's own answer is awaited");
        fresh.settle(&"b", now, Some((9, 100)));
        assert_eq!(fresh.get(&"b"), Some((9, 100)));
    }

    #[test]
    fn a_failure_keeps_the_context_there_was() {
        let now = Instant::now();
        let mut fresh = Fresh::default();
        assert!(fresh.want(&"a", 10, now));
        fresh.settle(&"a", now, Some((5, 100)));
        assert!(fresh.want(&"a", 20, now + CONTEXT_GAP));
        fresh.settle(&"a", now + CONTEXT_GAP, None);
        assert_eq!(fresh.get(&"a"), Some((5, 100)));
        assert!(!fresh.want(&"a", 30, now + CONTEXT_GAP), "waits a gap");
    }
}
