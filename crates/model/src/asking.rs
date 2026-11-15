//! What a session asks the user right now, as its agent's hooks told it.

use serde::{Deserialize, Deserializer, Serialize};

/// What a session asks the user, and since when.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Asking {
    /// When it began to ask, in milliseconds since the Unix epoch.
    pub since_ms: u64,
    pub asked: Asked,
}

/// What is asked of the user.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Asked {
    /// To let a tool run, and what the call runs or acts on.
    Tool { name: String, about: String },
    /// To answer the agent's questions, each with the options it offers.
    Questions { questions: Vec<Question> },
    /// To approve the plan the agent wrote.
    Plan { plan: String },
}

/// A question the agent asks the user to answer by choosing.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Question {
    /// A short label for it; empty when the agent gave none.
    #[serde(default)]
    pub header: String,
    pub question: String,
    pub options: Vec<Choice>,
    /// Whether more than one option may be chosen.
    #[serde(default)]
    pub multi_select: bool,
}

/// An option offered for a question.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Choice {
    pub label: String,
    #[serde(default)]
    pub description: String,
}

/// What a peer sent of what a session asks, or none when this build cannot
/// read it, as for a kind added later: the session is read all the same.
pub fn lenient<'de, D: Deserializer<'de>>(
    deserializer: D,
) -> Result<Option<Box<Asking>>, D::Error> {
    let value = Option::<serde_json::Value>::deserialize(deserializer)?;
    Ok(value.and_then(|value| serde_json::from_value(value).ok()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Session;

    #[test]
    fn a_session_asking_nothing_is_written_without_it() {
        let written = serde_json::to_value(Session::default()).unwrap();
        assert!(written.get("asking").is_none(), "{written}");
    }

    #[test]
    fn a_session_written_before_sessions_told_what_they_ask_reads_as_asking_nothing() {
        let mut written = serde_json::to_value(Session::default()).unwrap();
        written.as_object_mut().unwrap().remove("asking");
        let read: Session = serde_json::from_value(written).unwrap();
        assert_eq!(read.asking, None);
    }

    #[test]
    fn what_a_session_asks_reads_back_as_it_was_written() {
        let session = Session {
            asking: Some(Box::new(Asking {
                since_ms: 7,
                asked: Asked::Questions {
                    questions: vec![Question {
                        header: "Colour".into(),
                        question: "Pick a colour".into(),
                        options: vec![Choice {
                            label: "Red".into(),
                            description: "Choose red".into(),
                        }],
                        multi_select: false,
                    }],
                },
            })),
            ..Session::default()
        };
        let read: Session =
            serde_json::from_str(&serde_json::to_string(&session).unwrap()).unwrap();
        assert_eq!(read, session);
    }

    #[test]
    fn a_kind_of_asking_this_build_does_not_know_leaves_the_session_readable() {
        let mut written = serde_json::to_value(Session::default()).unwrap();
        written["asking"] = serde_json::json!({"since_ms": 1, "asked": {"kind": "elicitation"}});
        let read: Session = serde_json::from_value(written).unwrap();
        assert_eq!(read.asking, None);
    }
}
