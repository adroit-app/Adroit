# Keep scratch sessions in a directory of Adroit's own

Status: Accepted

## Context

Talking to an agent about something no project holds needs no directory, but
the agent still runs in one: it reads what is there and keeps the conversation
by it, so resuming works only from the same directory.

## Decision

A session started without a directory runs in a new directory of its own under
one root Adroit keeps in the user's data directory, listed as one place. Adroit
removes none of these directories.

## Alternatives considered

- A temporary directory. The system clears it, and the conversation can no
  longer be resumed.
- The home directory. The agent would read everything under it, and every such
  session would share it.
- One fixed directory for every such session. Their files would mix.

## Consequences

- The directories add up until the user removes them.
- The agent may ask to trust each new directory unless the root is trusted.
