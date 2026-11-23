//! What each session asks the user now, as its agent's hook tells of it for
//! as long as the agent waits to be answered.

use std::sync::{Arc, LazyLock, Mutex, MutexGuard};

use adroit_model::{MessageId, SessionId};
use serde::{Deserialize, Serialize};
use serde_json::Value;

/// What a hook tells the work host its agent asks.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Asking {
    /// Chosen by the hook, so that the question keeps it when it is told to
    /// the work host that follows.
    pub id: MessageId,
    /// The tool the agent asks to use, or asks through.
    pub tool: String,
    pub input: Value,
    /// The directory the agent works in as it asks.
    #[serde(default)]
    pub dir: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Kind {
    /// Whether a tool may be used.
    Permission,
    /// A question of the agent's own, with what may be chosen.
    Question,
    /// Whether to go on with a plan.
    Plan,
}

impl Kind {
    /// What Claude Code asks through `tool`.
    fn of(tool: &str) -> Self {
        match tool {
            "AskUserQuestion" => Self::Question,
            "ExitPlanMode" => Self::Plan,
            _ => Self::Permission,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Question {
    pub id: MessageId,
    pub kind: Kind,
    pub tool: String,
    pub input: Value,
    pub dir: Option<String>,
    pub asked_ms: u64,
}

/// The most questions of one session held at once: far more than an agent
/// asks, as each keeps a connection.
const HELD: usize = 32;

type Held = Arc<(SessionId, Question)>;

/// The questions that wait, the oldest first.
static WAITING: LazyLock<Mutex<Vec<Held>>> = LazyLock::new(Default::default);

fn waiting() -> MutexGuard<'static, Vec<Held>> {
    WAITING.lock().unwrap_or_else(|e| e.into_inner())
}

/// A question that waits for as long as this is held.
pub(crate) struct Waits(Held);

impl Drop for Waits {
    fn drop(&mut self) {
        waiting().retain(|held| !Arc::ptr_eq(held, &self.0));
    }
}

/// Holds what `session` asks, until what is returned is dropped.
/// None while the session has as many held as it may.
pub(crate) fn hold(session: SessionId, asking: Asking, now_ms: u64) -> Option<Waits> {
    let held = Arc::new((
        session,
        Question {
            id: asking.id,
            kind: Kind::of(&asking.tool),
            tool: asking.tool,
            input: asking.input,
            dir: asking.dir,
            asked_ms: now_ms,
        },
    ));
    let mut waiting = waiting();
    if waiting.iter().filter(|held| held.0 == session).count() >= HELD {
        return None;
    }
    waiting.push(held.clone());
    Some(Waits(held))
}

/// What `session` asks: the oldest that waits, which is the one its agent
/// shows.
pub(crate) fn of(session: SessionId) -> Option<Question> {
    waiting()
        .iter()
        .find(|held| held.0 == session)
        .map(|held| held.1.clone())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn asking(tool: &str) -> Asking {
        Asking {
            id: MessageId::new(),
            tool: tool.into(),
            input: json!({}),
            dir: None,
        }
    }

    #[test]
    fn a_question_waits_under_an_id_of_its_own_until_its_agent_goes_on() {
        let session = SessionId::new();
        let first = hold(session, asking("Bash"), 1).unwrap();
        let second = hold(session, asking("Edit"), 2).unwrap();
        let shown = of(session).unwrap();
        assert_eq!((shown.tool.as_str(), shown.asked_ms), ("Bash", 1));
        assert!(of(SessionId::new()).is_none());
        drop(first);
        let next = of(session).unwrap();
        assert_eq!(next.tool, "Edit");
        assert_ne!(next.id, shown.id);
        drop(second);
        assert!(of(session).is_none());
    }

    #[test]
    fn a_session_has_no_more_questions_held_than_it_may_and_keeps_no_other_from_asking() {
        let session = SessionId::new();
        let held: Vec<_> = (0..HELD)
            .map(|_| hold(session, asking("Bash"), 1).unwrap())
            .collect();
        assert!(hold(session, asking("Bash"), 1).is_none());
        assert!(hold(SessionId::new(), asking("Bash"), 1).is_some());
        drop(held);
        assert!(hold(session, asking("Bash"), 1).is_some());
    }

    #[test]
    fn a_question_and_a_plan_are_told_from_a_permission() {
        assert_eq!(Kind::of("AskUserQuestion"), Kind::Question);
        assert_eq!(Kind::of("ExitPlanMode"), Kind::Plan);
        assert_eq!(Kind::of("mcp__x__y"), Kind::Permission);
    }
}
