# Serve a work machine's own windows and its paired clients from the same hosts

Status: Accepted

## Context

Paired clients reached a work host and a terminal host the machine's services
ran apart from those its own windows used. Work begun at the machine could not
be reached from another computer, and work a paired client began there was
not the machine's own, so the user could not move between computers and go on
with the same sessions.

## Decision

Each instance runs one work host and one terminal host, which serve the
windows and commands of its machine and, through the gateway, its paired
clients alike. What a paired client asks is held to the rules of remote
changes by the connection it came on, not by the host it reached. The gateway
starts the hosts when none runs, and they outlive it.

## Alternatives considered

- Keep the services' hosts apart and let the machine's windows reach them as a
  paired client does. Work already running at the machine would stay out of
  reach, and the machine's own windows would lose what they do with local
  files.
- Let the services keep the hosts running and the windows use them. A window
  replacing a host of another build would fight the service manager restarting
  it.

## Consequences

- Turning paired clients away stops only the gateway; the work they began goes
  on as the machine's own.
- A host is found by paired clients through what it publishes as it starts,
  whoever started it.
- Moving from separate services ends the sessions their terminal host held.
