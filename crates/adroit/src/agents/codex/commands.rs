use adroit_claude::commands::Command;

fn skills(reply: &serde_json::Value, dir: &std::path::Path) -> Vec<Command> {
    let mut seen = std::collections::HashSet::new();
    reply["data"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|entry| {
            entry["cwd"]
                .as_str()
                .is_some_and(|cwd| std::path::Path::new(cwd) == dir)
        })
        .flat_map(|entry| entry["skills"].as_array().into_iter().flatten())
        .filter(|skill| skill["enabled"] == true)
        .filter_map(|skill| {
            let name = skill["name"]
                .as_str()
                .filter(|n| !n.is_empty() && !n.chars().any(char::is_whitespace))?;
            if !seen.insert(name) {
                return None;
            }
            let description = [
                &skill["interface"]["shortDescription"],
                &skill["shortDescription"],
                &skill["description"],
            ]
            .into_iter()
            .filter_map(|v| v.as_str())
            .find(|s| !s.is_empty())
            .unwrap_or(name);
            Some(Command {
                name: format!("${name}"),
                description: description.into(),
                argument_hint: String::new(),
            })
        })
        .collect()
}

pub(super) async fn probe(
    start: adroit_claude::Started,
    dir: &std::path::Path,
    timeout: std::time::Duration,
) -> Option<Vec<Command>> {
    let reply = super::rpc::request(
        start,
        dir,
        timeout,
        "skills/list",
        serde_json::json!({"cwds":[dir],"forceReload":true}),
    )
    .await?;
    reply["data"].as_array()?;
    Some(skills(&reply, dir))
}

// Codex exposes skills/list, but its TUI's built-in commands have no listing RPC.
// Keep this catalog in step with the CLI, excluding internal debug commands.
// https://developers.openai.com/codex/cli/slash-commands
pub(super) fn builtins() -> Vec<Command> {
    [
        ("model", "Choose the model and reasoning effort", "[model]"),
        ("permissions", "Change approval and sandbox permissions", ""),
        ("review", "Review changes", "[instructions]"),
        ("plan", "Switch to planning", "[prompt]"),
        ("goal", "Manage the task goal", "[goal]"),
        ("status", "Show session information", ""),
        ("usage", "Show account usage limits", ""),
        ("compact", "Reduce conversation context", ""),
        ("recap", "Summarize the conversation", ""),
        ("diff", "Inspect workspace changes", ""),
        ("new", "Start a conversation", "[name]"),
        ("clear", "Clear the terminal and start a conversation", ""),
        ("resume", "Reopen a conversation", ""),
        ("fork", "Branch the conversation", ""),
        ("rename", "Name the conversation", "[name]"),
        ("archive", "Archive the conversation", ""),
        ("delete", "Delete the conversation", ""),
        ("copy", "Copy a response", ""),
        ("export", "Export the conversation", "[path]"),
        ("mention", "Attach a file", "[path]"),
        ("init", "Create project instructions", ""),
        ("skills", "Browse available skills", ""),
        ("mcp", "Inspect MCP connections", "[verbose | login <name>]"),
        ("apps", "Browse connected apps", ""),
        ("plugins", "Browse plugins", ""),
        ("hooks", "Manage lifecycle hooks", ""),
        ("memories", "Configure memories", ""),
        ("config", "Configure Codex", ""),
        ("keymap", "Configure keyboard shortcuts", ""),
        ("vim", "Toggle composer Vim mode", ""),
        ("theme", "Choose syntax colors", ""),
        ("statusline", "Configure the status line", ""),
        ("title", "Configure the terminal title", ""),
        ("ide", "Include editor context", ""),
        ("agents", "Browse chats and agents", ""),
        ("subagents", "Switch to a subagent", ""),
        ("experimental", "Configure experimental features", ""),
        ("ps", "List background terminals", ""),
        ("stop", "Stop background terminals", ""),
        ("feedback", "Send feedback", ""),
        ("logout", "Sign out", ""),
        ("quit", "Close Codex", ""),
        ("exit", "Close Codex", ""),
    ]
    .into_iter()
    .map(|(name, description, argument_hint)| Command {
        name: name.into(),
        description: description.into(),
        argument_hint: argument_hint.into(),
    })
    .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::path::Path;

    #[test]
    fn enabled_skills_for_the_requested_directory_use_codex_invocations() {
        let reply = json!({"data":[
            {"cwd":"/api","skills":[
                {"name":"deploy","enabled":true,"description":"Full description","interface":{"shortDescription":"Deploy the service"}},
                {"name":"disabled","enabled":false,"description":"Disabled"},
                {"name":"","enabled":true},
                {"name":"deploy","enabled":true,"description":"Duplicate"}
            ]},
            {"cwd":"/other","skills":[{"name":"other","enabled":true}]}
        ]});
        assert_eq!(
            skills(&reply, Path::new("/api")),
            [Command {
                name: "$deploy".into(),
                description: "Deploy the service".into(),
                argument_hint: String::new()
            }]
        );
    }

    #[test]
    fn skill_descriptions_fall_back_and_missing_skills_add_nothing() {
        let reply = json!({"data":[{"cwd":"/api","skills":[
            {"name":"review","enabled":true,"shortDescription":"Review code","description":"Longer"},
            {"name":"test","enabled":true,"description":"Run tests"}
        ]}]});
        let got = skills(&reply, Path::new("/api"));
        assert_eq!(
            got.iter()
                .map(|c| c.description.as_str())
                .collect::<Vec<_>>(),
            ["Review code", "Run tests"]
        );
        assert!(skills(&json!({}), Path::new("/api")).is_empty());
    }

    #[tokio::test]
    async fn the_skill_probe_asks_for_the_directory_without_starting_a_thread() {
        use std::os::unix::fs::PermissionsExt;
        let dir = tempfile::tempdir().unwrap();
        let program = dir.path().join("codex");
        let reply = json!({"id":"adroit-query","result":{"data":[{"cwd":dir.path(),"skills":[{"name":"deploy","enabled":true,"description":"Deploy"}]}]}});
        std::fs::write(
            &program,
            format!(
                r#"#!/bin/sh
read line
echo '{{"id":"adroit-init","result":{{}}}}'
read line
IFS= read -r line
printf '%s\n' "$line" > query
echo '{reply}'
exec sleep 30
"#
            ),
        )
        .unwrap();
        std::fs::set_permissions(&program, std::fs::Permissions::from_mode(0o755)).unwrap();
        let got = probe(
            (program.to_str().unwrap().into(), vec![], vec![]),
            dir.path(),
            std::time::Duration::from_secs(5),
        )
        .await
        .unwrap();
        assert_eq!(got[0].name, "$deploy");
        let query: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(dir.path().join("query")).unwrap())
                .unwrap();
        assert_eq!(query["method"], "skills/list");
        assert_eq!(query["params"]["cwds"], json!([dir.path()]));
        assert_eq!(query["params"]["forceReload"], true);
    }

    #[tokio::test]
    async fn a_failed_skill_probe_does_not_replace_the_cached_list() {
        assert!(
            probe(
                ("/nonexistent/codex".into(), vec![], vec![]),
                Path::new("/"),
                std::time::Duration::from_secs(1)
            )
            .await
            .is_none()
        );
        assert!(builtins().iter().any(|c| c.name == "model"));
    }

    #[tokio::test]
    #[ignore = "needs Codex installed with enabled skills"]
    async fn installed_codex_lists_enabled_skills() {
        let got = probe(
            ("codex".into(), vec!["app-server".into()], vec![]),
            &std::env::temp_dir(),
            std::time::Duration::from_secs(15),
        )
        .await
        .expect("Codex skills");
        assert!(!got.is_empty());
        assert!(got.iter().all(|c| c.name.starts_with('$')));
    }
}
