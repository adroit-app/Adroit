# Let the user restart the session daemon after confirming

Status: Accepted

## Context

The daemon outlives the window, so after adroit is updated the window keeps
talking to a daemon of the build before. Replacing it ends every session it
holds.

## Decision

adroit restarts its daemon only when the user asks: from the command line, or
from the window after confirming that every live session ends. A window that
finds a daemon of another build offers the restart and does nothing more.

## Alternatives considered

- Restart a daemon of another build as soon as it is found. Live sessions end
  without the user choosing to.
- Upgrade the daemon in place. It cannot hand its sessions to a successor, and
  an old daemon cannot be taught to.
- Leave it to the user's own tools. Nothing tells them the daemon is out of
  date.

## Consequences

- The daemon has to say which build it is.
- Upgrading in place stays open once the daemon can hand over sessions.
