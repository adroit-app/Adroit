# Keep each instance of adroit apart, with one window each

Status: Accepted

## Context

A window and the daemon it starts share their sockets and the state they
keep. A second window opened on them could not answer the sessions, which
only one window can, and ran beside the first unaware of it. Yet a second
window is wanted at times: for work in another context altogether, whose
sessions should not mix with the first's, or to try a build of adroit beside
the one in use. Sessions outlive their window, so a window opened again has
to find the daemon and state of the one before it.

## Decision

adroit runs as named instances. Each instance has its own daemon, sockets
and state, and at most one window: opening its window again brings the open
one to the front. An instance is named when adroit is started and is the
default one otherwise, and every session and command started in an instance
belongs to it.

## Alternatives considered

- One window for all of adroit. Work in another context, and a build being
  tried, would share the sessions in use.
- A new instance each time the window opens. A window opened again would
  find none of the sessions left running by the one before it.
- Several windows on one instance. Only one of them can answer the sessions,
  and each would show and change the same sessions without the others
  knowing.

## Consequences

- A session sees only the sessions and shells of its own instance.
- Settings and keys are the same in every instance.
- The default instance keeps the daemon and state adroit kept before there
  were instances.
- A session started by an older adroit outside any instance belongs to the
  default one.
