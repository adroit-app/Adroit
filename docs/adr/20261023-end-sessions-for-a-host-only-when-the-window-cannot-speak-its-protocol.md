# End sessions for a host only when the window cannot speak its protocol

Status: Accepted

## Context

The hosts outlive the window, so after Adroit is updated the window meets
hosts of an earlier build, and once the window runs on another machine, hosts
of a build it never had. Replacing the terminal host ends every session it
holds; replacing the work host ends none. Most builds change nothing in how
the window and the hosts talk.

## Decision

Each protocol between the window and a host has a version, raised only when
what is sent changes shape. A host says which versions it speaks, and the
window works with it whenever they share one. A work host the window cannot
work with, or one of another build, is replaced without asking where the
window can start one. A terminal host the window cannot work with is replaced
only when the user asks, after confirming that every live session ends; until
then the window says so and does nothing more.

## Alternatives considered

- Treat a host of another build as one to replace. Every update asks the user
  to end their sessions, though the window could talk to the host.
- Replace the terminal host as soon as the window cannot work with it. Live
  sessions end without the user choosing to.
- Require the window and the hosts to be one build. A window on another
  machine could not be updated apart from the hosts.

## Consequences

- Changing the shape of anything a protocol sends needs a new version.
- A host has to say the versions it speaks before anything else.
- A terminal host from before protocols had versions says none, and is one the
  window cannot work with.
- A window on another machine cannot start the hosts' build, so there it
  reports a work host it cannot work with instead of replacing it.
