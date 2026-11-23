# Let a paired client have the gateway restart the work host

Status: Accepted

## Context

A work host that stops working, such as one waiting without end on a program,
can be put right only by replacing it, which ends no session. A paired client
could not ask for that, so a user away from the work machine had no way back
to their sessions short of going to it.

## Decision

The gateway restarts the work host itself when a paired client asks, on a
request of its own that says nothing else. It does so without the work host's
help. A request to stop the work host is still never relayed.

## Alternatives considered

- Relay the work protocol's request to stop. The gateway would start another
  host soon after, but a host that answers nothing would never take the
  request, and a client could leave the machine with no host.
- Have the work host restart itself. The one to be put right is the one that
  would have to act.
- Reach the machine by other means, such as a remote shell. It opens a second
  way in, with keys of its own, to do what a paired client already has the
  authority for.

## Consequences

- The work host can be restarted while it answers nothing or is not running.
- A gateway that is not running cannot help.
- A gateway from before this decision closes the request unanswered.
