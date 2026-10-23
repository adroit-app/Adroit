# Hold only the run whose record cannot be read

Status: Superseded by 20261023-give-every-session-the-same-means-to-delegate.md

## Context

The controller observes sessions, delivers messages and schedules workers from
one reading of the whole store. A single record that failed to decode failed
that reading, so every request of the instance stopped, and every session's
own reads failed with it, although the record belonged to one request.

## Decision

A record that cannot be read holds the run it belongs to, and only that run.
The controller leaves a held run untouched, keeps counting the worker slots it
occupies, and continues for every other run. The failure stays visible: reading
the held run, or the whole state at once, reports it. A record that belongs to
no run still fails every reading.

## Alternatives considered

- Skip only the unreadable record and keep scheduling its run. The rest of the
  run would be read without it, and a decision made on a partial run, such as
  starting a task whose dependency was skipped, may be wrong.
- Keep failing the whole store. One damaged request stops every other.

## Consequences

- One request can stay stopped while the others proceed, so a held run must be
  reported rather than silently left out.
- Changes to a held run fail when they touch its unreadable record, and nothing
  rewrites that record.
- A damaged record shared by several runs, such as a working area, still stops
  the controller.
