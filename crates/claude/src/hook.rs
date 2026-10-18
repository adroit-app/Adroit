//! Claude Code's hooks, which tell how a session stands, as the OSC 7501
//! program status its terminal keeps.

use std::path::Path;

use serde_json::{Value, json};

/// What a hook has the session's terminal told.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Report {
    /// The OSC 7501 report to write to the terminal, if the hook tells
    /// anything of the state.
    pub status: Option<Vec<u8>>,
    /// The conversation the session now holds, when the hook names one: it
    /// changes when the conversation is cleared or another one resumed.
    pub session_id: Option<String>,
}

fn osc(fields: &str) -> Vec<u8> {
    format!("\x1b]7501;{fields}:app=claude\x1b\\").into_bytes()
}

/// What the hook Claude Code ran with `event` reports.
pub fn report(event: &Value) -> Report {
    let name = event["hook_event_name"].as_str().unwrap_or_default();
    let tool = event["tool_name"].as_str().unwrap_or_default();
    let status = match name {
        "SessionStart" => Some("state=idle"),
        "UserPromptSubmit" | "PostToolUse" | "PostToolUseFailure" | "PermissionDenied"
        | "ElicitationResult" => Some("state=working"),
        "PreToolUse" if tool == "AskUserQuestion" => Some("state=blocked:kind=question"),
        "PermissionRequest" => Some("state=blocked:kind=permission"),
        "Elicitation" => Some("state=blocked:kind=question"),
        "Notification" => match event["notification_type"].as_str() {
            Some("permission_prompt") => Some("state=blocked:kind=permission"),
            Some("elicitation_dialog") => Some("state=blocked:kind=question"),
            _ => None,
        },
        "Stop" => Some("state=done"),
        "StopFailure" => Some("state=error"),
        "SessionEnd" => Some("state=clear"),
        _ => None,
    };
    Report {
        status: status.map(osc),
        session_id: (name == "SessionStart")
            .then(|| event["session_id"].as_str().map(str::to_string))
            .flatten(),
    }
}

/// The hooks a session adroit starts runs, each calling `adroit hook`.
pub const EVENTS: &[&str] = &[
    "SessionStart",
    "UserPromptSubmit",
    "PreToolUse",
    "PermissionRequest",
    "PermissionDenied",
    "PostToolUse",
    "PostToolUseFailure",
    "Elicitation",
    "ElicitationResult",
    "Notification",
    "Stop",
    "StopFailure",
    "SessionEnd",
];

/// The settings that have a session run `adroit` at `program` on each hook.
pub fn settings(program: &Path) -> String {
    let quoted = format!("'{}'", program.display().to_string().replace('\'', r"'\''"));
    let command =
        json!([{ "hooks": [{ "type": "command", "command": format!("{quoted} hook") }] }]);
    let hooks: serde_json::Map<String, Value> = EVENTS
        .iter()
        .map(|e| ((*e).to_string(), command.clone()))
        .collect();
    json!({ "hooks": hooks }).to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn status(event: Value) -> Option<String> {
        report(&event).status.map(|b| String::from_utf8(b).unwrap())
    }

    #[test]
    fn a_prompt_sent_and_a_tool_run_tell_the_session_works() {
        for name in ["UserPromptSubmit", "PostToolUse", "PermissionDenied"] {
            assert_eq!(
                status(json!({"hook_event_name": name})).as_deref(),
                Some("\x1b]7501;state=working:app=claude\x1b\\"),
                "{name}"
            );
        }
    }

    #[test]
    fn a_permission_asked_and_a_question_asked_tell_what_it_waits_on() {
        assert_eq!(
            status(json!({"hook_event_name": "PermissionRequest"})).as_deref(),
            Some("\x1b]7501;state=blocked:kind=permission:app=claude\x1b\\")
        );
        assert_eq!(
            status(json!({"hook_event_name": "PreToolUse", "tool_name": "AskUserQuestion"}))
                .as_deref(),
            Some("\x1b]7501;state=blocked:kind=question:app=claude\x1b\\")
        );
        assert_eq!(
            status(json!({"hook_event_name": "Notification", "notification_type": "elicitation_dialog"}))
                .as_deref(),
            Some("\x1b]7501;state=blocked:kind=question:app=claude\x1b\\")
        );
    }

    #[test]
    fn other_tools_and_other_notifications_tell_nothing() {
        assert_eq!(
            status(json!({"hook_event_name": "PreToolUse", "tool_name": "Bash"})),
            None
        );
        assert_eq!(
            status(json!({"hook_event_name": "Notification", "notification_type": "idle_prompt"})),
            None
        );
    }

    #[test]
    fn a_turn_ended_well_or_not_and_a_session_ended_are_told() {
        assert_eq!(
            status(json!({"hook_event_name": "Stop"})).as_deref(),
            Some("\x1b]7501;state=done:app=claude\x1b\\")
        );
        assert_eq!(
            status(json!({"hook_event_name": "StopFailure"})).as_deref(),
            Some("\x1b]7501;state=error:app=claude\x1b\\")
        );
        assert_eq!(
            status(json!({"hook_event_name": "SessionEnd"})).as_deref(),
            Some("\x1b]7501;state=clear:app=claude\x1b\\")
        );
    }

    #[test]
    fn a_session_starting_names_its_conversation() {
        let r = report(&json!({"hook_event_name": "SessionStart", "session_id": "abc"}));
        assert_eq!(r.session_id.as_deref(), Some("abc"));
        let r = report(&json!({"hook_event_name": "Stop", "session_id": "abc"}));
        assert_eq!(r.session_id, None);
    }

    #[test]
    fn every_hook_calls_adroit_by_its_path_quoted_for_the_shell() {
        let s: Value =
            serde_json::from_str(&settings(Path::new("/Applications/It's Adroit/adroit"))).unwrap();
        for e in EVENTS {
            assert_eq!(
                s["hooks"][e][0]["hooks"][0]["command"],
                r"'/Applications/It'\''s Adroit/adroit' hook",
                "{e}"
            );
        }
    }
}
