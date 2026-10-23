# Enforce Claude Code read-only through its sandbox and denied editing tools

Status: Accepted

## Context

A session can be started read-only, for investigation or review, whichever
agent runs it. Claude Code's plan mode does not stop writes, and a read-only
session works in the live source directory, so the restriction has to hold
whatever permission mode it runs in and whatever the user's or repository's
settings allow.

## Decision

A read-only Claude Code session is launched with its file-editing tools denied
and with command-line settings that run its shell commands in Claude Code's
operating-system sandbox, denying writes to its directory and the history
behind it, refusing retries outside the sandbox, and refusing to start without
the sandbox.

## Alternatives considered

- Deny the editing tools alone. A shell command writes files just as well.
- Deny the shell too. Investigation relies on read-only commands.
- Offer read-only for other agents only. Read-only review would need another
  agent.

## Consequences

- Command-line settings rank above the user's and the repository's, and denials
  only accumulate, so those files cannot loosen it.
- Where the sandbox cannot run, the session fails to start instead of running
  unconfined.
- Commands the user excluded from the sandbox, and tools outside the shell such
  as MCP servers and hooks, are not confined.
