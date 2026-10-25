# List every session by its directory

Status: Accepted

## Context

Work starts with a conversation in a directory, and what it becomes shows only
later, so asking for a project first leaves nowhere to begin. Every session
runs in a directory, and git knows its repository and worktree. A session
opened by another stands on its own and answers to the user, usually in a
worktree of its own.

## Decision

adroit keeps no project file. Every session, shell, editor and file is listed
under the directory it belongs to: its repository's worktree, or the directory
itself outside git. A session is listed the same way whoever started it.

## Alternatives considered

- Projects, filed at the start or later. Filing is a record to keep in step,
  named before the work is understood.
- List a session opened by another under the session that opened it. It reads
  as a part of that session's work, which it is not, and moves when that
  session ends.
- A second view grouped by who opened whom. The user has to pick a view, and
  each lists the same sessions.

## Consequences

- Grouping costs a git lookup per directory.
- Where a session came from is told where the session is read, not by where it
  is listed.
