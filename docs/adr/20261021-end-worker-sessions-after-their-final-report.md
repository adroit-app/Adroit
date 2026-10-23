# End Worker sessions after their final report

Status: Superseded by 20261023-end-a-started-session-once-it-reports.md

## Context

Finished Workers retained native processes and file descriptors indefinitely.
Manually stopped Leaders also remained in navigation as recovery entries, so
closing a request did not release its sessions or remove it from view.

## Decision

End Worker sessions after their final report. Treat manually closing a Leader
as ending its request and its Workers. Keep durable results independently of
live sessions, and offer recovery only through an explicit control operation.

## Alternatives considered

- Preserve finished Workers for follow-up messages. Idle sessions accumulate
  resources without a bound; follow-up work can receive a new attempt.
- Retain stopped Leaders as recovery entries. A deliberate close must remove
  the session rather than require another cleanup action.

## Consequences

- Workers must finish verification and save evidence before reporting.
- Reports and working files remain available after native sessions end.
- Unexpected Leader exits remain distinct from deliberate closure.
