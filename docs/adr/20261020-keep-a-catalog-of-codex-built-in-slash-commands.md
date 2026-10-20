# Keep a catalog of Codex built-in slash commands

Status: Accepted

## Decision

Adroit keeps a catalog of Codex's built-in slash commands for the embedded
prompt. Codex remains responsible for deciding whether a command is available
in the running session.

## Alternatives considered

- Ask Codex for the list. Its app-server exposes skills, but no list of
  the TUI's built-in slash commands.
- Read the TUI's slash menu. Starting a terminal session only to scrape a
  menu depends on startup dialogs and rendering, and creates a conversation.
- Leave completion unavailable. It makes basic commands hard to find when
  writing in the embedded prompt.

## Consequences

Codex upgrades may require catalog updates. Completion does not reflect
session-specific feature gates or discover custom commands. It starts no
process and works before a session starts.

## Context

Claude Code can list its commands through a control request. Codex keeps
its built-in slash commands in the TUI, apart from the app-server protocol,
so the same discovery mechanism cannot serve both agents.
