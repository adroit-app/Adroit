//! What each session asks the user now, as its agent's hook tells of it for
//! as long as the agent waits to be answered.

use std::sync::{Arc, LazyLock, Mutex, MutexGuard};
use std::time::Duration;

use adroit_model::{MessageId, SessionId};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tokio::sync::oneshot;

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

/// What a question is decided as.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "decision", rename_all = "snake_case")]
pub enum Decision {
    /// Allowed this once, with `input` in place of what was asked when the
    /// answer is part of it.
    Allow {
        input: Option<Value>,
    },
    Deny {
        message: Option<String>,
    },
}

/// The tool a question whether to trust a folder is told as asked through: it
/// is asked through none, as Claude Code asks it before it runs any.
pub(crate) const TRUST: &str = "TrustFolder";

/// A decision on its way to the agent, and what is told once it is there, or
/// why it is not.
pub(crate) type Decided = (Decision, oneshot::Sender<Result<(), String>>);

/// What became of a decision.
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum Taken {
    Yes,
    /// The question is not asked any more.
    NotAsked,
    Failed(String),
}

/// How long an agent's hook is given to take a decision.
const TAKES: Duration = Duration::from_secs(5);

struct Asked {
    session: SessionId,
    question: Question,
    /// Taken by the first decision.
    decide: Mutex<Option<oneshot::Sender<Decided>>>,
}

type Held = Arc<Asked>;

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

/// Holds what `session` asks, until what is returned is dropped, with what it
/// is decided as from outside, if it is;
/// none while the session has as many held as it may.
pub(crate) fn hold(
    session: SessionId,
    asking: Asking,
    now_ms: u64,
) -> Option<(Waits, oneshot::Receiver<Decided>)> {
    let (decide, decided) = oneshot::channel();
    let held = Arc::new(Asked {
        session,
        decide: Mutex::new(Some(decide)),
        question: Question {
            id: asking.id,
            kind: Kind::of(&asking.tool),
            tool: asking.tool,
            input: asking.input,
            dir: asking.dir,
            asked_ms: now_ms,
        },
    });
    let mut waiting = waiting();
    if waiting
        .iter()
        .filter(|held| held.session == session)
        .count()
        >= HELD
    {
        return None;
    }
    waiting.push(held.clone());
    Some((Waits(held), decided))
}

/// What `session` asks: the oldest that waits, which is the one its agent
/// shows.
pub(crate) fn of(session: SessionId) -> Option<Question> {
    waiting()
        .iter()
        .find(|held| held.session == session)
        .map(|held| held.question.clone())
}

/// The one question `session` asks by `id`; none when it asks no such one, or
/// several under that id, which no agent does of itself.
fn one(session: SessionId, id: MessageId) -> Option<Held> {
    let waiting = waiting();
    let mut named = waiting
        .iter()
        .filter(|held| held.session == session && held.question.id == id);
    let first = named.next().cloned();
    first.filter(|_| named.next().is_none())
}

/// The question `session` asks by `id`.
pub(crate) fn named(session: SessionId, id: MessageId) -> Option<Question> {
    one(session, id).map(|held| held.question.clone())
}

/// Decides the question `session` asks by `id`; whether the agent took the
/// decision, which it does not once the user answered.
pub(crate) async fn answer(session: SessionId, id: MessageId, decision: Decision) -> Taken {
    let decide = one(session, id)
        .and_then(|held| held.decide.lock().unwrap_or_else(|e| e.into_inner()).take());
    let Some(decide) = decide else {
        return Taken::NotAsked;
    };
    let (took, taken) = oneshot::channel();
    if decide.send((decision, took)).is_err() {
        return Taken::NotAsked;
    }
    match tokio::time::timeout(TAKES, taken).await {
        Ok(Ok(Ok(()))) => Taken::Yes,
        Ok(Ok(Err(why))) => Taken::Failed(why),
        Ok(Err(_)) | Err(_) => Taken::NotAsked,
    }
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
        let (first, _) = hold(session, asking("Bash"), 1).unwrap();
        let (second, _) = hold(session, asking("Edit"), 2).unwrap();
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

    const ALLOW: Decision = Decision::Allow { input: None };

    #[tokio::test]
    async fn a_decision_reaches_the_question_named_once() {
        let session = SessionId::new();
        let (waits, decided) = hold(session, asking("Bash"), 1).unwrap();
        let id = of(session).unwrap().id;
        assert_eq!(
            answer(session, MessageId::new(), ALLOW).await,
            Taken::NotAsked
        );
        assert_eq!(answer(SessionId::new(), id, ALLOW).await, Taken::NotAsked);
        let took = tokio::spawn(answer(session, id, ALLOW));
        let (decision, taken) = decided.await.unwrap();
        assert_eq!(decision, ALLOW);
        taken.send(Ok(())).unwrap();
        assert_eq!(took.await.unwrap(), Taken::Yes);
        let again = answer(session, id, ALLOW).await;
        assert_eq!(again, Taken::NotAsked, "decided already");
        drop(waits);
    }

    #[tokio::test]
    async fn a_decision_the_agent_no_longer_waits_for_is_not_taken() {
        let session = SessionId::new();
        let (waits, decided) = hold(session, asking("Bash"), 1).unwrap();
        let id = of(session).unwrap().id;
        let took = tokio::spawn(answer(session, id, ALLOW));
        drop(decided.await.unwrap());
        assert_eq!(took.await.unwrap(), Taken::NotAsked);
        drop(waits);
        assert_eq!(answer(session, id, ALLOW).await, Taken::NotAsked);
    }

    #[tokio::test]
    async fn questions_that_share_an_id_are_answered_by_none() {
        let session = SessionId::new();
        let first = asking("Bash");
        let _one = hold(session, first.clone(), 1).unwrap();
        let _two = hold(session, first.clone(), 2).unwrap();
        assert_eq!(answer(session, first.id, ALLOW).await, Taken::NotAsked);
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
