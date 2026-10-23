# List sessions by their directory, and a started session under its starter

Status: Accepted

## Context

Work starts with a conversation in a directory, and what it becomes shows only
later, so asking for a project first leaves nowhere to begin. Every session
runs in a directory, and git knows its repository and worktree. Sessions that
start sessions usually put them in worktrees of their own, so listing those by
directory scatters one piece of work across the tree.

## Decision

adroit keeps no project file. A session the user started, and every shell,
editor and file, is listed under the directory it belongs to: its repository's
worktree, or the directory itself outside git. A session started by another is
listed under the session that started it, wherever it runs, while that session
is listed.

## Alternatives considered

- Projects, filed at the start or later. Filing is a record to keep in step,
  named before the work is understood.
- List every session by directory and mark who started whom. One session's
  work spreads over every worktree it used.
- A second view grouped by who started whom. The user has to pick a view, and
  each lists the same sessions.

## Consequences

- Sessions running in other terminals sit beside adroit's own.
- Grouping costs a git lookup per directory.
- A session whose starter has gone is listed by its directory again.
