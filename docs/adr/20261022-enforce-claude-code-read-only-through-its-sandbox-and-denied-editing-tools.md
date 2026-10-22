# Enforce Claude Code read-only through its sandbox and denied editing tools

Status: Accepted

## Context

The read-only permission profile was refused for Claude Code, because plan
mode does not stop writes. Leaders were still offered the profile for either
agent and chose a combination that could never start. A read-only Worker
works in the live source directory, so the profile has to hold whatever
permission mode the session runs in and whatever the user's or the
repository's settings allow.

## Decision

A read-only Claude Code session is launched with its file-editing tools
denied and with settings, passed on its command line, that run its shell
commands in Claude Code's operating-system sandbox. The sandbox denies writes
to the session's directory and to the git history behind it, refuses the
retry outside the sandbox, and stops Claude Code from starting when the
sandbox is unavailable.

## Alternatives considered

- Deny the editing tools alone. A shell command writes files just as well.
- Deny the shell too. Investigation and review rely on read-only commands
  such as searching and reading history.
- Keep refusing the profile for Claude Code. Leaders cannot tell the
  combination apart from a supported one, and read-only review would need
  Codex.

## Consequences

- Command-line settings rank above the user's and the repository's files, and
  denials only accumulate across them, so those files cannot loosen it.
- Where the sandbox cannot run, a read-only Claude Code session fails to start
  instead of running unconfined.
- Commands the user has excluded from the sandbox in their own settings still
  run outside it.
- Tools that do not run through the shell, such as MCP servers and hooks, are
  outside the sandbox.
