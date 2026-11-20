//! What Claude Code's hooks tell of a session as its turn goes: what it asks
//! the user now, and what it did in its latest turns.
//!
//! The events are Claude Code's own and unversioned, so they are read
//! tolerantly: fields this reader does not know are passed over, and an
//! event missing what it needs is ignored.

use adroit_model::asking::{Asked, Asking, Choice, Question};
use adroit_model::steps::{Answer, Outcome, Step, Turn};
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

/// How many turns of a session are remembered, the one going on among them,
/// the oldest forgotten first.
const TURNS: usize = 5;

/// How many steps of a turn are remembered: its prompt, and the latest of
/// the rest.
const TURN_STEPS: usize = 64;

/// A turn, with the tool call each of its steps is, when it is one.
#[derive(Debug, Clone)]
struct Kept {
    turn: Turn,
    calls: Vec<Option<String>>,
}

/// What one session's hooks have told so far of what it did in its latest
/// turns.
#[derive(Debug, Clone, Default)]
pub struct Turns {
    kept: Vec<Kept>,
}

impl Turns {
    /// The turns remembered, the latest last.
    pub fn turns(&self) -> Vec<Turn> {
        self.kept.iter().map(|kept| kept.turn.clone()).collect()
    }

    /// Take in the hook `event`, heard at `now_ms` since the Unix epoch.
    pub fn hear(&mut self, event: &Value, now_ms: u64) {
        let tool = event["tool_name"].as_str();
        let call = event["tool_use_id"].as_str();
        let input = &event["tool_input"];
        match event["hook_event_name"].as_str() {
            Some("UserPromptSubmit") => {
                self.end(now_ms);
                self.begin(now_ms);
                if let Some(prompt) = text(&event["prompt"]) {
                    self.push(Step::Prompted(prompt), None, now_ms);
                }
            }
            Some("PreToolUse") => {
                if let Some(tool) = tool {
                    let step = Step::Tool {
                        name: tool.to_owned(),
                        about: call_about(tool, input),
                        outcome: None,
                    };
                    self.push(step, call, now_ms);
                }
            }
            Some(ended @ ("PostToolUse" | "PostToolUseFailure" | "PermissionDenied")) => {
                let Some(tool) = tool else {
                    return;
                };
                let outcome = match ended {
                    "PostToolUse" => Outcome::Done,
                    "PostToolUseFailure" => Outcome::Failed,
                    _ => Outcome::Denied,
                };
                let step = match answers(input) {
                    Some(answers) if outcome == Outcome::Done => Step::Answered(answers),
                    _ => Step::Tool {
                        name: tool.to_owned(),
                        about: call_about(tool, input),
                        outcome: Some(outcome),
                    },
                };
                self.settle(call, step, now_ms);
            }
            Some("Stop") => {
                if let Some(said) = text(&event["last_assistant_message"]) {
                    self.push(Step::Said(said), None, now_ms);
                }
                self.end(now_ms);
            }
            Some("SessionStart") if event["source"] == "clear" => self.kept.clear(),
            _ => {}
        }
    }

    fn begin(&mut self, now_ms: u64) {
        self.kept.push(Kept {
            turn: Turn {
                started_ms: now_ms,
                ended_ms: None,
                steps: Vec::new(),
            },
            calls: Vec::new(),
        });
        let over = self.kept.len().saturating_sub(TURNS);
        self.kept.drain(..over);
    }

    fn end(&mut self, now_ms: u64) {
        if let Some(kept) = self.kept.last_mut() {
            kept.turn.ended_ms.get_or_insert(now_ms);
        }
    }

    /// The turn going on, begun at `now_ms` when none is, as a turn heard
    /// from its middle on, once the work host came up during it.
    fn current(&mut self, now_ms: u64) -> &mut Kept {
        if self
            .kept
            .last()
            .is_none_or(|kept| kept.turn.ended_ms.is_some())
        {
            self.begin(now_ms);
        }
        self.kept.last_mut().expect("a turn was just begun")
    }

    fn push(&mut self, step: Step, call: Option<&str>, now_ms: u64) {
        let kept = self.current(now_ms);
        kept.turn.steps.push(step);
        kept.calls.push(call.map(str::to_owned));
        if kept.turn.steps.len() > TURN_STEPS {
            // The prompt tells what the turn is for, so it outlives the
            // steps after it.
            let at = usize::from(matches!(kept.turn.steps[0], Step::Prompted(_)));
            kept.turn.steps.remove(at);
            kept.calls.remove(at);
        }
    }

    /// The step of `call` replaced by how it ended, or `step` taken in as
    /// one of its own when the call's beginning was not heard.
    fn settle(&mut self, call: Option<&str>, step: Step, now_ms: u64) {
        let found = call.and_then(|call| {
            self.kept.iter_mut().rev().find_map(|kept| {
                let at = kept.calls.iter().position(|c| c.as_deref() == Some(call))?;
                Some(&mut kept.turn.steps[at])
            })
        });
        match found {
            Some(was) => *was = step,
            None => self.push(step, call, now_ms),
        }
    }
}

/// What a call to `tool` with `input` acts on, as an approval of it is
/// told: an answer's first question, a plan's title.
fn call_about(tool: &str, input: &Value) -> String {
    match asked(tool, input) {
        Asked::Tool { about, .. } => about,
        Asked::Questions { questions } => questions
            .first()
            .map(|q| clip(&q.question))
            .unwrap_or_default(),
        Asked::Plan { plan } => plan
            .lines()
            .map(|l| l.trim_start_matches('#').trim())
            .find(|l| !l.is_empty())
            .map(clip)
            .unwrap_or_default(),
    }
}

/// The answers given to the questions `input` asks, in the order asked;
/// none when it holds no answers.
fn answers(input: &Value) -> Option<Vec<Answer>> {
    let given = input["answers"].as_object()?;
    let answer = |question: &str| {
        let answer = match &given.get(question)? {
            Value::String(label) => label.clone(),
            Value::Array(labels) => labels
                .iter()
                .filter_map(Value::as_str)
                .collect::<Vec<_>>()
                .join(", "),
            _ => return None,
        };
        Some(Answer {
            question: clip(question),
            answer: clip(&answer),
        })
    };
    let asked: Vec<&str> = input["questions"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|q| q["question"].as_str())
        .collect();
    let mut out: Vec<Answer> = asked.iter().filter_map(|q| answer(q)).collect();
    out.extend(
        given
            .keys()
            .filter(|q| !asked.contains(&q.as_str()))
            .filter_map(|q| answer(q)),
    );
    Some(out)
}

/// `value`'s text, trimmed and clipped; none when it holds none.
fn text(value: &Value) -> Option<String> {
    value
        .as_str()
        .map(str::trim)
        .filter(|text| !text.is_empty())
        .map(clip)
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
    Asked::Tool {
        name: tool.to_owned(),
        about: about(input, RUNS),
    }
}

// What will run, or what it runs on, comes before the agent's own words of
// it: the user approves the call, not its description.
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

    fn prompt(text: &str) -> Value {
        json!({"session_id": "e16c", "prompt_id": "49c4", "permission_mode": "default",
            "hook_event_name": "UserPromptSubmit", "prompt": text})
    }

    fn stop(said: &str) -> Value {
        json!({"session_id": "e16c", "hook_event_name": "Stop", "stop_hook_active": false,
            "last_assistant_message": said, "background_tasks": [], "session_crons": []})
    }

    fn did(events: &[Value]) -> Vec<Turn> {
        let mut turns = Turns::default();
        for (at, event) in events.iter().enumerate() {
            turns.hear(event, at as u64);
        }
        turns.turns()
    }

    fn tool(name: &str, about: &str, outcome: Option<Outcome>) -> Step {
        Step::Tool {
            name: name.into(),
            about: about.into(),
            outcome,
        }
    }

    #[test]
    fn a_turn_lists_its_prompt_its_calls_its_answers_and_what_was_said_at_its_end() {
        let mut answered = colour();
        answered["answers"] = json!({"Pick a colour": "Red"});
        let turns = did(&[
            prompt("Stamp the date"),
            pre("Bash", bash(), "t1"),
            request("Bash", bash()),
            post("PostToolUse", "Bash", bash(), "t1"),
            pre("AskUserQuestion", colour(), "t2"),
            request("AskUserQuestion", colour()),
            post("PostToolUse", "AskUserQuestion", answered, "t2"),
            stop("Stamped; you picked Red."),
        ]);
        assert_eq!(
            turns,
            [Turn {
                started_ms: 0,
                ended_ms: Some(7),
                steps: vec![
                    Step::Prompted("Stamp the date".into()),
                    tool("Bash", "date > stamp.txt", Some(Outcome::Done)),
                    Step::Answered(vec![Answer {
                        question: "Pick a colour".into(),
                        answer: "Red".into()
                    }]),
                    Step::Said("Stamped; you picked Red.".into()),
                ],
            }]
        );
    }

    #[test]
    fn a_call_not_yet_ended_has_no_outcome_and_one_ended_tells_how() {
        let turns = did(&[
            prompt("Go"),
            pre("Bash", bash(), "t1"),
            pre("Bash", json!({"command": "ls"}), "t2"),
            pre("Bash", json!({"command": "rm -rf /"}), "t3"),
            pre("Read", json!({"file_path": "/w/a.rs"}), "t4"),
            post("PostToolUseFailure", "Bash", bash(), "t1"),
            post(
                "PermissionDenied",
                "Bash",
                json!({"command": "rm -rf /"}),
                "t3",
            ),
            post("PostToolUse", "Read", json!({"file_path": "/w/a.rs"}), "t4"),
        ]);
        assert_eq!(
            turns[0].steps[1..],
            [
                tool("Bash", "date > stamp.txt", Some(Outcome::Failed)),
                tool("Bash", "ls", None),
                tool("Bash", "rm -rf /", Some(Outcome::Denied)),
                tool("Read", "/w/a.rs", Some(Outcome::Done)),
            ]
        );
        assert_eq!(turns[0].ended_ms, None);
    }

    #[test]
    fn a_call_whose_beginning_was_not_heard_is_listed_once_it_ends() {
        let turns = did(&[post("PostToolUse", "Bash", bash(), "t1")]);
        assert_eq!(
            turns[0].steps,
            [tool("Bash", "date > stamp.txt", Some(Outcome::Done))]
        );
    }

    #[test]
    fn a_call_is_about_what_an_approval_of_it_would_be_about() {
        let turns = did(&[
            pre("AskUserQuestion", colour(), "t1"),
            pre(
                "ExitPlanMode",
                json!({"plan": "# Stamp\n\nWrite it."}),
                "t2",
            ),
            pre(
                "Agent",
                json!({"description": "Explore", "prompt": "List."}),
                "t3",
            ),
        ]);
        assert_eq!(
            turns[0].steps,
            [
                tool("AskUserQuestion", "Pick a colour", None),
                tool("ExitPlanMode", "Stamp", None),
                tool("Agent", "Explore", None),
            ]
        );
    }

    #[test]
    fn several_answers_are_listed_in_the_order_their_questions_were_asked() {
        let input = json!({
            "questions": [{"question": "Size?"}, {"question": "Colours?"}],
            "answers": {"Colours?": ["Red", "Blue"], "Size?": "Large"}
        });
        let turns = did(&[post("PostToolUse", "AskUserQuestion", input, "t1")]);
        let answer = |question: &str, answer: &str| Answer {
            question: question.into(),
            answer: answer.into(),
        };
        assert_eq!(
            turns[0].steps,
            [Step::Answered(vec![
                answer("Size?", "Large"),
                answer("Colours?", "Red, Blue")
            ])]
        );
    }

    #[test]
    fn each_prompt_begins_a_turn_and_ends_the_one_before() {
        let turns = did(&[prompt("One"), prompt("Two"), stop("Done.")]);
        assert_eq!(turns.len(), 2);
        assert_eq!(turns[0].ended_ms, Some(1));
        assert_eq!(turns[1].steps[0], Step::Prompted("Two".into()));
        assert_eq!(turns[1].ended_ms, Some(2));
    }

    #[test]
    fn a_step_after_the_turn_ended_begins_another_without_a_prompt() {
        let turns = did(&[prompt("One"), stop(""), pre("Bash", bash(), "t1")]);
        assert_eq!(turns.len(), 2);
        assert_eq!(turns[0].steps, [Step::Prompted("One".into())]);
        assert_eq!(turns[1].started_ms, 2);
        assert_eq!(turns[1].steps, [tool("Bash", "date > stamp.txt", None)]);
    }

    #[test]
    fn only_the_latest_turns_are_remembered() {
        let prompts: Vec<Value> = (0..TURNS + 2).map(|n| prompt(&n.to_string())).collect();
        let turns = did(&prompts);
        assert_eq!(turns.len(), TURNS);
        assert_eq!(turns[0].steps, [Step::Prompted("2".into())]);
    }

    #[test]
    fn a_long_turn_keeps_its_prompt_and_its_latest_steps() {
        let mut events = vec![prompt("Go")];
        events.extend(
            (0..TURN_STEPS + 3)
                .map(|n| pre("Read", json!({"file_path": n.to_string()}), &n.to_string())),
        );
        let turns = did(&events);
        let steps = &turns[0].steps;
        assert_eq!(steps.len(), TURN_STEPS);
        assert_eq!(steps[0], Step::Prompted("Go".into()));
        assert_eq!(steps[1], tool("Read", "4", None));
    }

    #[test]
    fn long_text_of_a_turn_is_clipped() {
        let turns = did(&[prompt(&"あ".repeat(RESPONSE)), stop(&"い".repeat(RESPONSE))]);
        for step in &turns[0].steps {
            let (Step::Prompted(text) | Step::Said(text)) = step else {
                panic!("{step:?}");
            };
            assert!(text.len() < RESPONSE * 2, "{}", text.len());
        }
    }

    #[test]
    fn a_cleared_conversation_forgets_its_turns() {
        let cleared = json!({"hook_event_name": "SessionStart", "source": "clear"});
        assert!(did(&[prompt("One"), stop("Done."), cleared]).is_empty());
    }

    #[test]
    fn events_that_tell_nothing_done_leave_the_turns_as_they_were() {
        let turns = did(&[
            prompt("Go"),
            request("Bash", bash()),
            event("MessageDisplay"),
            event("Notification"),
            json!({"no": "event"}),
            json!({"hook_event_name": "PreToolUse"}),
        ]);
        assert_eq!(turns[0].steps, [Step::Prompted("Go".into())]);
    }
}
