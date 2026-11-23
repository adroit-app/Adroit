//! The steps an agent takes: a subagent's, as its transcript tells, and a
//! session's in its latest turns, as its agent's hooks told the work host.

use serde::{Deserialize, Deserializer, Serialize};

/// A step an agent took, or one taken in its turn.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum Step {
    /// A tool it called, what the call was about, and how the call ended
    /// once that is known.
    Tool {
        name: String,
        about: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        outcome: Option<Outcome>,
    },
    /// What it said, or handed back as its report.
    Said(String),
    /// The prompt it was sent.
    Prompted(String),
    /// The questions it asked, as they were answered.
    Answered(Vec<Answer>),
}

/// How a tool call ended.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Outcome {
    Done,
    Failed,
    /// The user, or a rule, did not let it run.
    Denied,
}

/// A question and the option chosen, or the options joined.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Answer {
    pub question: String,
    pub answer: String,
}

/// One turn of a session: from the prompt it was sent until it stopped.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Turn {
    /// When it began, in milliseconds since the Unix epoch.
    pub started_ms: u64,
    /// When it ended; none while it goes on.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ended_ms: Option<u64>,
    /// Its steps, the last last.
    #[serde(deserialize_with = "readable_steps")]
    pub steps: Vec<Step>,
}

/// The steps a peer sent that this build can read, the others passed over,
/// as a kind of step added later.
fn readable_steps<'de, D: Deserializer<'de>>(deserializer: D) -> Result<Vec<Step>, D::Error> {
    let values = Vec::<serde_json::Value>::deserialize(deserializer)?;
    Ok(values
        .into_iter()
        .filter_map(|value| serde_json::from_value(value).ok())
        .collect())
}

/// What a peer sent of a session's turns, or none when this build cannot
/// read them: what carries them is read all the same.
pub fn lenient<'de, D: Deserializer<'de>>(deserializer: D) -> Result<Option<Vec<Turn>>, D::Error> {
    let value = Option::<serde_json::Value>::deserialize(deserializer)?;
    Ok(value.and_then(|value| serde_json::from_value(value).ok()))
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn a_turn_reads_back_as_it_was_written() {
        let turn = Turn {
            started_ms: 1,
            ended_ms: Some(9),
            steps: vec![
                Step::Prompted("Stamp the date".into()),
                Step::Tool {
                    name: "Bash".into(),
                    about: "date > stamp.txt".into(),
                    outcome: Some(Outcome::Done),
                },
                Step::Answered(vec![Answer {
                    question: "Pick a colour".into(),
                    answer: "Red".into(),
                }]),
                Step::Said("Done.".into()),
            ],
        };
        let read: Turn = serde_json::from_str(&serde_json::to_string(&turn).unwrap()).unwrap();
        assert_eq!(read, turn);
    }

    #[test]
    fn a_tool_step_without_an_outcome_is_written_as_a_subagent_step_was() {
        let step = Step::Tool {
            name: "Read".into(),
            about: "a.rs".into(),
            outcome: None,
        };
        assert_eq!(
            serde_json::to_value(&step).unwrap(),
            json!({"Tool": {"name": "Read", "about": "a.rs"}})
        );
    }

    #[test]
    fn a_kind_of_step_this_build_does_not_know_is_passed_over_and_the_rest_kept() {
        let read: Turn = serde_json::from_value(json!({
            "started_ms": 1,
            "steps": [{"Prompted": "hi"}, {"Thought": "hmm"}, {"Said": "hello"}]
        }))
        .unwrap();
        assert_eq!(
            read.steps,
            [Step::Prompted("hi".into()), Step::Said("hello".into())]
        );
    }
}
