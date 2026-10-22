# Separate every managed session's launch from its readiness

Status: Accepted; supersedes
[20261022-separate-worker-process-launch-from-readiness.md](20261022-separate-worker-process-launch-from-readiness.md).

## Context

A terminal can be running while its agent waits on a dialog: a trust or
permission question, or a warning shown at every launch. Only Workers were
watched, so a Leader held at such a dialog looked started. A dialog the
adapter could not name looked like a slow start until it was reported as
stalled.

## Decision

Every managed session, Leader or Worker, has its operational readiness kept
apart from its process state. The provider adapter interprets its own screen,
and any open dialog counts as waiting for the user, whether or not the adapter
can name it. Orchestration records and notifies readiness transitions. Adroit
never answers provider trust or permission questions for the user.

## Alternatives considered

- Treat a live terminal as ready. This conceals decisions that keep the session
  from beginning work.
- Watch Workers only. A Leader held at startup stops its whole request without
  telling anyone.
- Report only the dialogs the adapter recognises and leave the rest to the
  stall timeout. The user learns late, and wrongly, that the startup stalled
  rather than that it waits for them.

## Consequences

- A Worker retains its attempt slot while it requires human action.
- A Leader's readiness is reported as a Worker's is.
- Readiness is observable without changing who owns the terminal or request.
