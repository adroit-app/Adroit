use adroit_claude::commands::Command;

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
