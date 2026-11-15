//! What Claude Code's hooks tell of a session as its turn goes: what it asks
//! the user now.
//!
//! The events are Claude Code's own and unversioned, so they are read
//! tolerantly: fields this reader does not know are passed over, and an
//! event missing what it needs is ignored.

use adroit_model::asking::{Asked, Asking, Choice, Question};
use serde_json::Value;

use super::subagents::{about, clip};

/// How long a `tool_response` may run, written as JSON, before it is left
/// out of what a hook hands on.
const RESPONSE: usize = 4096;

/// How many tool calls a session is remembered to be making at once, the
/// oldest forgotten first.
const CALLS: usize = 32;

/// `event` as a hook hands it on to the work host: its long text clipped and
/// a long tool output left out, as a tool's output can run to megabytes.
pub fn bounded(mut event: Value) -> Value {
    if let Some(fields) = event.as_object_mut()
        && fields
            .get("tool_response")
            .is_some_and(|response| response.to_string().len() > RESPONSE)
    {
        fields.remove("tool_response");
    }
    clipped(&mut event);
    event
}

fn clipped(value: &mut Value) {
    match value {
        Value::String(text) => {
            if text.len() > RESPONSE {
                *text = clip(text);
            }
        }
        Value::Array(items) => items.iter_mut().for_each(clipped),
        Value::Object(fields) => fields.values_mut().for_each(clipped),
        _ => {}
    }
}

/// A tool call the session has begun and not yet ended.
#[derive(Debug, Clone)]
struct Call {
    id: String,
    tool: String,
    input: Value,
}

/// What the user is asked, with the call it is asked for when that is known.
#[derive(Debug, Clone)]
struct Open {
    call: Option<String>,
    tool: String,
    asking: Asking,
}

/// What one session's hooks have told so far of what it asks the user.
#[derive(Debug, Clone, Default)]
pub struct Asks {
    calls: Vec<Call>,
    asked: Option<Open>,
}

impl Asks {
    /// What the session asks the user now.
    pub fn asking(&self) -> Option<&Asking> {
        self.asked.as_ref().map(|asked| &asked.asking)
    }

    /// Forget what the session asks, as once it is no longer shown asking.
    pub fn clear(&mut self) {
        self.asked = None;
    }

    /// Take in the hook `event`, heard at `now_ms` since the Unix epoch.
    pub fn hear(&mut self, event: &Value, now_ms: u64) {
        let tool = event["tool_name"].as_str();
        let call = event["tool_use_id"].as_str();
        match event["hook_event_name"].as_str() {
            Some("PreToolUse") => {
                if let (Some(tool), Some(id)) = (tool, call) {
                    self.calls.push(Call {
                        id: id.to_owned(),
                        tool: tool.to_owned(),
                        input: event["tool_input"].clone(),
                    });
                    let over = self.calls.len().saturating_sub(CALLS);
                    self.calls.drain(..over);
                }
            }
            Some("PermissionRequest") => {
                let (Some(tool), Some(input)) = (tool, event.get("tool_input")) else {
                    return;
                };
                // The request names no call, so it is paired with the latest
                // call to begin with the same tool and input.
                let call = self
                    .calls
                    .iter()
                    .rev()
                    .find(|c| c.tool == tool && &c.input == input)
                    .map(|c| c.id.clone());
                self.asked = Some(Open {
                    call,
                    tool: tool.to_owned(),
                    asking: Asking {
                        since_ms: now_ms,
                        asked: asked(tool, input),
                    },
                });
            }
            Some("PostToolUse" | "PostToolUseFailure" | "PermissionDenied") => {
                if let Some(id) = call {
                    self.calls.retain(|c| c.id != id);
                }
                let ends = self
                    .asked
                    .as_ref()
                    .is_some_and(|asked| match (&asked.call, call) {
                        (Some(asked), Some(ended)) => asked == ended,
                        _ => Some(asked.tool.as_str()) == tool,
                    });
                if ends {
                    self.asked = None;
                }
            }
            Some("Stop" | "UserPromptSubmit") => *self = Self::default(),
            _ => {}
        }
    }
}

/// What the user is asked to let `tool` do with `input`.
fn asked(tool: &str, input: &Value) -> Asked {
    match tool {
        "AskUserQuestion" => {
            let questions: Vec<Question> = input["questions"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(question)
                .collect();
            if !questions.is_empty() {
                return Asked::Questions { questions };
            }
        }
        "ExitPlanMode" => {
            return Asked::Plan {
                plan: input["plan"].as_str().unwrap_or_default().to_owned(),
            };
        }
        _ => {}
    }
    // What will run, or what it runs on, comes before the agent's own words
    // of it: the user approves the call, not its description.
    const RUNS: &[&str] = &[
        "command",
        "file_path",
        "notebook_path",
        "url",
        "pattern",
        "path",
        "query",
        "skill",
        "to",
        "description",
        "prompt",
    ];
    Asked::Tool {
        name: tool.to_owned(),
        about: about(input, RUNS),
    }
}

fn question(value: &Value) -> Option<Question> {
    let text = |key: &str| value[key].as_str().unwrap_or_default().to_owned();
    Some(Question {
        header: text("header"),
        question: value["question"].as_str()?.to_owned(),
        options: value["options"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|option| {
                Some(Choice {
                    label: option["label"].as_str()?.to_owned(),
                    description: option["description"]
                        .as_str()
                        .unwrap_or_default()
                        .to_owned(),
                })
            })
            .collect(),
        multi_select: value["multiSelect"].as_bool().unwrap_or(false),
    })
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    // Events as Claude Code 2.1.325 hands them to its hooks, trimmed of the
    // paths and ids nothing here reads.

    fn bash() -> Value {
        json!({"command": "date > stamp.txt", "description": "Write current date to stamp file"})
    }

    fn colour() -> Value {
        json!({"questions": [{"question": "Pick a colour", "header": "Colour", "options": [
            {"label": "Red", "description": "Choose red"},
            {"label": "Blue", "description": "Choose blue"}
        ], "multiSelect": false}]})
    }

    fn pre(tool: &str, input: Value, id: &str) -> Value {
        json!({"session_id": "e16c", "permission_mode": "default", "effort": {"level": "medium"},
            "hook_event_name": "PreToolUse", "tool_name": tool, "tool_input": input, "tool_use_id": id})
    }

    fn request(tool: &str, input: Value) -> Value {
        json!({"session_id": "e16c", "permission_mode": "default", "effort": {"level": "medium"},
            "hook_event_name": "PermissionRequest", "tool_name": tool, "tool_input": input,
            "permission_suggestions": [{"type": "addDirectories", "directories": ["/w"], "destination": "session"}]})
    }

    fn post(event: &str, tool: &str, input: Value, id: &str) -> Value {
        json!({"session_id": "e16c", "hook_event_name": event, "tool_name": tool, "tool_input": input,
            "tool_response": {"stdout": "", "stderr": "", "interrupted": false}, "tool_use_id": id, "duration_ms": 159})
    }

    fn event(name: &str) -> Value {
        json!({"session_id": "e16c", "hook_event_name": name})
    }

    fn heard(events: &[Value]) -> Asks {
        let mut asks = Asks::default();
        for (at, event) in events.iter().enumerate() {
            asks.hear(event, at as u64);
        }
        asks
    }

    fn asked_of(events: &[Value]) -> Option<Asked> {
        heard(events).asking().map(|asking| asking.asked.clone())
    }

    #[test]
    fn a_command_waiting_for_approval_is_asked_as_the_command_it_runs() {
        let asks = heard(&[pre("Bash", bash(), "t1"), request("Bash", bash())]);
        assert_eq!(
            asks.asking(),
            Some(&Asking {
                since_ms: 1,
                asked: Asked::Tool {
                    name: "Bash".into(),
                    about: "date > stamp.txt".into(),
                },
            })
        );
    }

    #[test]
    fn a_tool_waiting_for_approval_is_asked_as_what_it_acts_on_before_its_description() {
        let cases = [
            (
                "Write",
                json!({"file_path": "/w/a.rs", "content": "fn main() {}"}),
                "/w/a.rs",
            ),
            (
                "WebFetch",
                json!({"url": "https://example.com", "prompt": "Summarise it"}),
                "https://example.com",
            ),
            (
                "Grep",
                json!({"pattern": "fn main", "description": "Find the entry point"}),
                "fn main",
            ),
            (
                "Agent",
                json!({"description": "Explore the tree", "prompt": "List it."}),
                "Explore the tree",
            ),
            (
                "Bash",
                json!({"command": "\n  cargo test\n  cargo clippy"}),
                "cargo test",
            ),
        ];
        for (tool, input, about) in cases {
            assert_eq!(
                asked_of(&[request(tool, input)]),
                Some(Asked::Tool {
                    name: tool.into(),
                    about: about.into()
                }),
                "{tool}"
            );
        }
    }

    #[test]
    fn a_question_is_asked_with_its_options() {
        assert_eq!(
            asked_of(&[
                pre("AskUserQuestion", colour(), "t1"),
                request("AskUserQuestion", colour())
            ]),
            Some(Asked::Questions {
                questions: vec![Question {
                    header: "Colour".into(),
                    question: "Pick a colour".into(),
                    options: vec![
                        Choice {
                            label: "Red".into(),
                            description: "Choose red".into()
                        },
                        Choice {
                            label: "Blue".into(),
                            description: "Choose blue".into()
                        },
                    ],
                    multi_select: false,
                }],
            })
        );
    }

    #[test]
    fn a_plan_is_asked_to_be_approved_with_its_text() {
        let plan = json!({"plan": "# Stamp\n\nWrite the date."});
        assert_eq!(
            asked_of(&[request("ExitPlanMode", plan)]),
            Some(Asked::Plan {
                plan: "# Stamp\n\nWrite the date.".into()
            })
        );
    }

    #[test]
    fn a_plan_whose_text_is_not_given_is_asked_all_the_same() {
        assert_eq!(
            asked_of(&[request("ExitPlanMode", json!({}))]),
            Some(Asked::Plan {
                plan: String::new()
            })
        );
    }

    #[test]
    fn questions_that_cannot_be_read_are_asked_as_the_tool_they_come_from() {
        assert_eq!(
            asked_of(&[request("AskUserQuestion", json!({"questions": "?"}))]),
            Some(Asked::Tool {
                name: "AskUserQuestion".into(),
                about: String::new()
            })
        );
    }

    #[test]
    fn the_call_ending_ends_what_it_asked() {
        for ended in ["PostToolUse", "PostToolUseFailure", "PermissionDenied"] {
            let asks = heard(&[
                pre("Bash", bash(), "t1"),
                request("Bash", bash()),
                post(ended, "Bash", bash(), "t1"),
            ]);
            assert_eq!(asks.asking(), None, "{ended}");
        }
    }

    #[test]
    fn an_answered_question_ends_though_its_input_now_holds_the_answers() {
        let mut answered = colour();
        answered["answers"] = json!({"Pick a colour": "Red"});
        let asks = heard(&[
            pre("AskUserQuestion", colour(), "t1"),
            request("AskUserQuestion", colour()),
            post("PostToolUse", "AskUserQuestion", answered, "t1"),
        ]);
        assert_eq!(asks.asking(), None);
    }

    #[test]
    fn another_call_of_the_same_tool_ending_leaves_the_approval_asked() {
        let other = json!({"command": "ls"});
        let asks = heard(&[
            pre("Bash", other.clone(), "t1"),
            pre("Bash", bash(), "t2"),
            request("Bash", bash()),
            post("PostToolUse", "Bash", other, "t1"),
        ]);
        assert!(asks.asking().is_some());
    }

    #[test]
    fn an_approval_whose_call_is_not_known_ends_with_any_call_of_its_tool() {
        let asks = heard(&[
            request("Bash", bash()),
            post("PostToolUse", "Bash", bash(), "t9"),
        ]);
        assert_eq!(asks.asking(), None);
    }

    #[test]
    fn the_turn_ending_or_a_prompt_arriving_ends_what_was_asked() {
        for name in ["Stop", "UserPromptSubmit"] {
            let asks = heard(&[request("Bash", bash()), event(name)]);
            assert_eq!(asks.asking(), None, "{name}");
        }
    }

    #[test]
    fn events_that_tell_nothing_asked_leave_it_as_it_was() {
        let notification = json!({"hook_event_name": "Notification",
            "message": "Claude needs your permission", "notification_type": "permission_prompt"});
        let asks = heard(&[
            request("Bash", bash()),
            notification,
            event("MessageDisplay"),
            json!({"no": "event"}),
        ]);
        assert!(asks.asking().is_some());
    }

    #[test]
    fn a_request_missing_its_tool_is_ignored() {
        let asks = heard(&[json!({"hook_event_name": "PermissionRequest", "tool_input": {}})]);
        assert_eq!(asks.asking(), None);
    }

    #[test]
    fn a_long_tool_output_is_left_out_of_what_a_hook_hands_on() {
        let mut event = post("PostToolUse", "Bash", bash(), "t1");
        event["tool_response"] = json!({"stdout": "x".repeat(RESPONSE * 4)});
        let handed = bounded(event);
        assert!(handed.get("tool_response").is_none());
        assert_eq!(handed["tool_use_id"], "t1");
    }

    #[test]
    fn a_short_tool_output_is_handed_on_whole() {
        let event = post("PostToolUse", "Bash", bash(), "t1");
        assert_eq!(bounded(event.clone()), event);
    }

    #[test]
    fn long_text_is_clipped_alike_wherever_it_is_handed_on_so_calls_still_pair() {
        let long = json!({"content": "あ".repeat(RESPONSE), "file_path": "/w/a"});
        let (pre, request) = (
            bounded(pre("Write", long.clone(), "t1")),
            bounded(request("Write", long)),
        );
        assert!(pre.to_string().len() < RESPONSE * 2);
        assert_eq!(pre["tool_input"], request["tool_input"]);
        let asks = heard(&[pre, request]);
        assert_eq!(asks.asked.as_ref().unwrap().call.as_deref(), Some("t1"));
    }
}
