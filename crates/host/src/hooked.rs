//! What the hooks of each session Adroit started told the work host of what
//! it asks the user and what it did in its latest turns, kept in memory while
//! the work host runs.

use std::collections::HashMap;
use std::sync::{LazyLock, Mutex};

use adroit_agents::claude::hooks::{Asks, Turns};
use adroit_model::asking::Asking;
use adroit_model::steps::Turn;
use adroit_model::{Agent, SessionId};
use serde_json::Value;

/// What one session's hooks told, with when it was last seen waiting.
#[derive(Debug, Default)]
struct Heard {
    asks: Asks,
    turns: Turns,
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
        let heard = self.0.entry(record).or_default();
        heard.asks.hear(event, now_ms);
        heard.turns.hear(event, now_ms);
    }

    fn turns(&self, record: SessionId) -> Vec<Turn> {
        self.0
            .get(&record)
            .map(|heard| heard.turns.turns())
            .unwrap_or_default()
    }

    fn asking(&mut self, record: SessionId, waiting: bool, now_ms: u64) -> Option<Asking> {
        self.0
            .get_mut(&record)
            .and_then(|heard| heard.seen(waiting, now_ms))
    }

    fn asks(&self, record: SessionId) -> Option<&Asking> {
        self.0.get(&record).and_then(|heard| heard.asks.asking())
    }

    /// Forget `asking` once it was answered, unless the session already
    /// asks something else.
    fn answered(&mut self, record: SessionId, asking: &Asking) {
        if let Some(heard) = self.0.get_mut(&record)
            && heard.asks.asking() == Some(asking)
        {
            heard.asks.clear();
        }
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

/// What the session `record` asks the user as its hooks last told, judged by
/// no screen.
pub(crate) fn asks(record: SessionId) -> Option<Asking> {
    heard().asks(record).cloned()
}

/// Forget that the session `record` asks `asking`, now that it was answered.
pub(crate) fn answered(record: SessionId, asking: &Asking) {
    heard().answered(record, asking);
}

/// What the session `record` did in its latest turns, the latest last.
pub(crate) fn turns(record: SessionId) -> Vec<Turn> {
    heard().turns(record)
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
        assert!(s.turns(ended).is_empty());
    }

    #[test]
    fn the_same_hooks_tell_what_a_session_asks_and_what_its_turn_did() {
        let mut s = Sessions::default();
        let record = SessionId::new();
        s.hear(
            record,
            Agent::Claude,
            &json!({"hook_event_name": "UserPromptSubmit", "prompt": "Stamp it"}),
            1,
        );
        request(&mut s, record, 2);
        assert!(s.asking(record, true, 3).is_some());
        assert_eq!(
            s.turns(record)[0].steps,
            [adroit_model::steps::Step::Prompted("Stamp it".into())]
        );
    }

    #[test]
    fn a_question_answered_is_asked_no_more() {
        let mut s = Sessions::default();
        let record = SessionId::new();
        request(&mut s, record, 10);
        let asked = s.asks(record).cloned().unwrap();
        s.answered(record, &asked);
        assert_eq!(s.asks(record), None);
    }

    #[test]
    fn answering_a_question_asked_before_leaves_the_one_asked_since() {
        let mut s = Sessions::default();
        let record = SessionId::new();
        request(&mut s, record, 10);
        let before = s.asks(record).cloned().unwrap();
        request(&mut s, record, 20);
        s.answered(record, &before);
        assert_eq!(s.asks(record).map(|a| a.since_ms), Some(20));
    }

    #[test]
    fn a_session_whose_hooks_told_nothing_did_nothing_known() {
        assert!(Sessions::default().turns(SessionId::new()).is_empty());
    }
}
