# Let a started session ask its starter to divide the work

Status: Accepted

## Context

A session started by another can find its work larger than it looked: parts
that could run apart, or parts better suited to another agent. Starting
sessions of its own would put work further from the user and from the session
that answers for it, and nothing would bound how many sessions follow.

## Decision

A session started by another session cannot start sessions. When its work
outgrows it, it tells the session that started it what it has found and how
it would divide the rest, and that session decides whether and how to start
more.

## Alternatives considered

- Let started sessions start sessions, to a fixed depth. Work divided below
  the first level is decided where neither the user nor the starting session
  sees the whole of it.
- Let started sessions start sessions freely. Sessions multiply without bound
  and their reports travel through sessions that may have ended.

## Consequences

- Every session is at most one step from a session the user started.
- A session asked to divide the work can start new sessions from what the
  asking session has already done.
- A started session can still use its agent's own subagents.
