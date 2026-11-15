//! What the hooks of each session Adroit started told the work host of what
//! it asks the user, kept in memory while the work host runs.

use std::collections::HashMap;
use std::sync::{LazyLock, Mutex};

use adroit_agents::claude::hooks::Asks;
use adroit_model::asking::Asking;
use adroit_model::{Agent, SessionId};
use serde_json::Value;

/// What one session's hooks told, with when it was last seen waiting.
#[derive(Debug, Default)]
struct Heard {
    asks: Asks,
    waited_ms: Option<u64>,
}

impl Heard {
    /// What is asked now that the session's screen and status read as
    /// `waiting` at `now_ms`. A question asked before the session was last
    /// seen waiting, and no longer waited on, was answered at its terminal
    /// without a hook telling so.
    fn seen(&mut self, waiting: bool, now_ms: u64) -> Option<Asking> {
        if waiting {
            self.waited_ms = Some(now_ms);
        } else if let (Some(asking), Some(waited)) = (self.asks.asking(), self.waited_ms)
            && asking.since_ms <= waited
        {
            self.asks.clear();
        }
        self.asks.asking().cloned()
    }
}

/// What the hooks of every session told, by its record.
#[derive(Debug, Default)]
struct Sessions(HashMap<SessionId, Heard>);

impl Sessions {
    fn hear(&mut self, record: SessionId, agent: Agent, event: &Value, now_ms: u64) {
        if agent != Agent::Claude {
            return;
        }
        self.0.entry(record).or_default().asks.hear(event, now_ms);
    }

    fn asking(&mut self, record: SessionId, waiting: bool, now_ms: u64) -> Option<Asking> {
        self.0
            .get_mut(&record)
            .and_then(|heard| heard.seen(waiting, now_ms))
    }

    fn keep_only(&mut self, kept: &[SessionId]) {
        self.0.retain(|record, _| kept.contains(record));
    }
}

static HEARD: LazyLock<Mutex<Sessions>> = LazyLock::new(Default::default);

fn heard() -> std::sync::MutexGuard<'static, Sessions> {
    HEARD.lock().unwrap_or_else(|e| e.into_inner())
}

/// Take in what a hook of the session `record`, running `agent`, was handed
/// at `now_ms`; an agent whose hooks are not read is passed over.
pub(crate) fn hear(record: SessionId, agent: Agent, event: &Value, now_ms: u64) {
    heard().hear(record, agent, event, now_ms);
}

/// What the session `record` asks the user, now that it reads as `waiting`
/// at `now_ms`.
pub(crate) fn asking(record: SessionId, waiting: bool, now_ms: u64) -> Option<Asking> {
    heard().asking(record, waiting, now_ms)
}

/// Forgets every session but those `kept`, as the others ended.
pub(crate) fn keep_only(kept: &[SessionId]) {
    heard().keep_only(kept);
}

#[cfg(test)]
mod tests {
    use adroit_model::asking::Asked;
    use serde_json::json;

    use super::*;

    fn request(sessions: &mut Sessions, record: SessionId, at: u64) {
        sessions.hear(
            record,
            Agent::Claude,
            &json!({"hook_event_name": "PermissionRequest", "tool_name": "Bash",
                "tool_input": {"command": "date > stamp.txt"}}),
            at,
        );
    }

    #[test]
    fn a_session_asks_what_its_hooks_last_told() {
        let mut s = Sessions::default();
        let record = SessionId::new();
        request(&mut s, record, 10);
        assert_eq!(
            s.asking(record, true, 20).map(|a| a.asked),
            Some(Asked::Tool {
                name: "Bash".into(),
                about: "date > stamp.txt".into()
            })
        );
    }

    #[test]
    fn a_question_seen_waited_on_ends_once_the_session_no_longer_waits() {
        let mut s = Sessions::default();
        let record = SessionId::new();
        request(&mut s, record, 10);
        assert!(s.asking(record, true, 20).is_some());
        assert_eq!(s.asking(record, false, 30), None);
    }

    #[test]
    fn a_question_asked_before_its_dialog_is_drawn_is_kept() {
        let mut s = Sessions::default();
        let record = SessionId::new();
        request(&mut s, record, 10);
        assert!(s.asking(record, false, 12).is_some());
        assert!(s.asking(record, true, 14).is_some());
    }

    #[test]
    fn a_question_asked_after_the_last_wait_is_kept_though_the_screen_reads_working() {
        let mut s = Sessions::default();
        let record = SessionId::new();
        request(&mut s, record, 10);
        assert!(s.asking(record, true, 20).is_some());
        request(&mut s, record, 25);
        assert!(s.asking(record, false, 30).is_some());
    }

    #[test]
    fn the_hooks_of_agents_not_read_are_passed_over() {
        let mut s = Sessions::default();
        let record = SessionId::new();
        s.hear(
            record,
            Agent::Codex,
            &json!({"hook_event_name": "PermissionRequest", "tool_name": "Bash", "tool_input": {}}),
            1,
        );
        assert_eq!(s.asking(record, true, 2), None);
    }

    #[test]
    fn a_session_that_ended_is_forgotten() {
        let mut s = Sessions::default();
        let (ended, running) = (SessionId::new(), SessionId::new());
        request(&mut s, ended, 1);
        request(&mut s, running, 1);
        s.keep_only(&[running]);
        assert_eq!(s.asking(ended, true, 2), None);
    }
}
