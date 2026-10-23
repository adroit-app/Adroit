# Show a started session under the session that started it

Status: Accepted; supersedes
20261018-find-sessions-by-the-directory-they-run-in-keeping-no-project-file.md
for sessions started by sessions.

## Context

Sessions were listed by the directory they run in, and a session's own
delegation went to Claude Code's agents inside it. Sessions now start sessions
of their own, usually in worktrees of their own, so listing them by directory
scatters one piece of work across the tree and puts it beside unrelated
sessions that happen to share a worktree. A separate Leader / Workers view
listed them by request instead, which the user had to switch to.

## Decision

A session started by another is listed under the session that started it,
wherever it runs. Sessions the user started are still listed by the directory
they run in, as
20261018-find-sessions-by-the-directory-they-run-in-keeping-no-project-file.md
decided. There is one sidebar; no view lists sessions by role or request.

## Alternatives considered

- List every session by its directory and mark which started which. The work
  of one session is spread over every worktree its sessions run in.
- Keep a second view grouped by who started whom. The user has to choose a
  view to see where work stands, and the two views list the same sessions
  twice.

## Consequences

- A session's row shows where it runs when that is not where its starter runs.
- A worktree lists only the sessions the user started there.
- A session whose starter has ended is listed by its directory again.
