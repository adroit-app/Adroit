# Keep each instance of adroit apart, with hosts of its own

Status: Proposed

## Context

A second set of sessions is wanted at times: for work in another context whose
sessions should not mix, or to try a build beside the one in use. Sessions
outlive their window, so a window opened again has to find them. The work and
the sessions are held by the hosts, and the window only shows them.

## Decision

adroit runs as named instances, the default one unless another is named at
start. Each has a terminal host, a work host and stored state of its own, and
every session and command started in an instance belongs to it. Any number of
windows may show an instance, and each shows what its work host holds.

## Alternatives considered

- One set of hosts for all of adroit. Another context, or a build being tried,
  would share the sessions in use.
- A new instance each time a window opens. It would find none of the sessions
  left running.
- At most one window to an instance. The same work could not be shown from two
  machines, and nothing the window holds needs it to be alone.

## Consequences

- A session sees only the sessions and shells of its own instance.
- Two windows on one instance see each other's changes to the work, and keep
  how they show it apart.
- Settings and keys are shared by every instance.
