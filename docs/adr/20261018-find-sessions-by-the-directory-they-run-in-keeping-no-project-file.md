# Find sessions by the directory they run in, keeping no project file

Status: Accepted

## Context

Work starts with a conversation in a directory, and what it becomes shows only
later. Asking for a project first, with a name, a goal and the worktrees it
would be done in, leaves nowhere to begin. Every session already runs in a
directory, and git knows the repository and worktree of each directory.

## Decision

adroit keeps no project file. It groups sessions by the directory each runs
in: by the git repository holding it, divided into that repository's
worktrees, or by the directory itself outside git. Starting a session asks
only for that directory.

## Alternatives considered

- Keep projects, but let a session start outside one and be filed later.
  Filing would be a record to keep in step with the sessions, and the project
  would still have to be named before it is understood.
- Record which session started which, to draw sessions as parents and
  children. The work a session hands on goes to Claude Code's own agents,
  which already record theirs.
- Start sessions in a scratch directory and move them once the work is
  clear. A Claude Code session is bound to the directory it starts in: its
  conversation, instructions and settings are found from there.

## Consequences

- A session is told nothing by adroit before its first prompt.
- Sessions running in other terminals sit beside adroit's own in the same
  directory.
- Grouping costs a git lookup per directory a session runs in.
