# Separate Worker process launch from operational readiness

Status: Superseded by
[20261022-separate-every-managed-session-s-launch-from-its-readiness.md](20261022-separate-every-managed-session-s-launch-from-its-readiness.md)

## Context

A terminal can be running while its agent waits for the user's trust or
permission decision. Treating that terminal as ready hides the action from the
user and from the Leader.

## Decision

Worker process state and operational readiness are separate. The provider
adapter interprets its own terminal screen, while orchestration records and
notifies readiness transitions. Adroit never answers provider trust or
permission questions for the user.

## Alternatives considered

- Treat a live terminal as a ready Worker. This conceals decisions that prevent
  the Worker from beginning work.
- Automatically accept startup dialogs. This would take security decisions away
  from the user.

## Consequences

- A Worker can retain its attempt slot while requiring human action.
- Readiness is observable without changing who owns the terminal or request.
