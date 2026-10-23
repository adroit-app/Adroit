# Keep each instance of adroit apart, with one window each

Status: Accepted

## Context

A window and its daemon share sockets and state, and only one window can answer
the sessions. Yet a second window is wanted at times: for work in another
context whose sessions should not mix, or to try a build beside the one in use.
Sessions outlive their window, so a window opened again has to find them.

## Decision

adroit runs as named instances, the default one unless another is named at
start. Each has its own daemon, sockets and state, and at most one window:
opening it again brings the open one forward. Every session and command
started in an instance belongs to it.

## Alternatives considered

- One window for all of adroit. Another context, or a build being tried, would
  share the sessions in use.
- A new instance each time the window opens. It would find none of the sessions
  left running.
- Several windows on one instance. Only one can answer the sessions, and each
  would change them unaware of the others.

## Consequences

- A session sees only the sessions and shells of its own instance.
- Settings and keys are shared by every instance.
