//! The `/` commands Pi offers: those its resources add, asked of Pi, and
//! its interface's own, which it lists nowhere.

use std::path::Path;
use std::time::Duration;

use serde_json::Value;

use crate::commands::Command;

/// The commands `data` lists: those extensions, prompt templates and skills
/// add, by the name typed after `/`.
fn listed(data: &Value) -> Option<Vec<Command>> {
    Some(
        data["commands"]
            .as_array()?
            .iter()
            .filter_map(|c| {
                let name = c["name"]
                    .as_str()
                    .filter(|n| !n.is_empty() && !n.chars().any(char::is_whitespace))?;
                Some(Command {
                    name: name.into(),
                    description: c["description"].as_str().unwrap_or_default().into(),
                    argument_hint: String::new(),
                })
            })
            .collect(),
    )
}

pub(super) async fn probe(
    start: crate::Started,
    dir: &Path,
    timeout: Duration,
) -> Option<Vec<Command>> {
    listed(&super::rpc::request(start, dir, timeout, "get_commands").await?)
}

// Pi's interface handles its built-in commands itself and lists them only in
// its documentation; keep this in step with Pi's slash commands.
pub(super) fn builtins() -> Vec<Command> {
    [
        ("settings", "Open settings", ""),
        ("model", "Select a model", "[provider/model]"),
        ("thinking", "Set the thinking level", "[level]"),
        (
            "scoped-models",
            "Configure the models used by interactive cycling",
            "",
        ),
        ("login", "Add provider authentication", "[provider]"),
        ("logout", "Remove provider authentication", ""),
        ("new", "Start a new session", ""),
        ("resume", "Switch to another saved session", ""),
        ("name", "Set the session display name", "[name]"),
        ("session", "Show session information and statistics", ""),
        ("tree", "Navigate the session tree", ""),
        (
            "fork",
            "Create a new session from an earlier user message",
            "",
        ),
        ("clone", "Duplicate the current session", ""),
        ("compact", "Compact the current context", "[instructions]"),
        ("import", "Import and resume a JSONL session", "<path>"),
        ("copy", "Copy the last assistant message", ""),
        ("export", "Export the session as HTML or JSONL", "[path]"),
        ("share", "Upload the session and return a viewer link", ""),
        ("trust", "Save a project trust decision", ""),
        (
            "reload",
            "Reload extensions, skills, templates and context files",
            "",
        ),
        ("hotkeys", "Show active keyboard shortcuts", ""),
        ("changelog", "Show changelog entries", ""),
        ("quit", "Quit Pi", ""),
    ]
    .into_iter()
    .map(|(name, description, hint)| Command {
        name: name.into(),
        description: description.into(),
        argument_hint: hint.into(),
    })
    .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::{program, started};
    use serde_json::json;

    #[test]
    fn the_commands_resources_add_are_listed_by_their_name() {
        let got = listed(&json!({"commands": [
            {"name": "fix-tests", "description": "Fix failing tests", "source": "prompt"},
            {"name": "skill:start-session", "source": "skill"},
            {"name": "two words", "source": "extension"},
        ]}))
        .unwrap();
        assert_eq!(
            got.iter()
                .map(|c| (c.name.as_str(), c.description.as_str()))
                .collect::<Vec<_>>(),
            [
                ("fix-tests", "Fix failing tests"),
                ("skill:start-session", "")
            ]
        );
        assert_eq!(listed(&json!({})), None);
    }

    #[tokio::test]
    async fn the_probe_asks_rpc_mode_for_its_commands() {
        let (dir, pi) = program(
            "pi",
            r#"IFS= read -r line
printf '%s' "$line" > request
echo '{"type":"response","id":"adroit","command":"get_commands","success":true,"data":{"commands":[{"name":"fix"}]}}'
exec sleep 30"#,
        );
        let got = probe(started(&pi, &[]), dir.path(), Duration::from_secs(5)).await;
        assert_eq!(got.map(|c| c.len()), Some(1));
        let request = std::fs::read_to_string(dir.path().join("request")).unwrap();
        assert!(request.contains("\"get_commands\""), "{request}");
    }

    #[test]
    fn the_interface_s_own_commands_include_naming_and_logging_in() {
        let names: Vec<String> = builtins().into_iter().map(|c| c.name).collect();
        for name in ["name", "login", "model", "new", "resume"] {
            assert!(names.iter().any(|n| n == name), "{name}");
        }
    }
}
