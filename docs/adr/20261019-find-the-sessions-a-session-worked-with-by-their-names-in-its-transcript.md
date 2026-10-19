# Find the sessions a session worked with by their names in its transcript

Status: Accepted

## Context

Sessions hand work to one another and message each other through Claude
Code, and the user following one wants to reach the others it works with.
Which session started which is kept in the daemon's tags. Claude Code keeps
no record of whom a session messaged except its transcript, and that names
each session only by the name Claude Code addresses it by: no conversation
id, which adroit could match exactly.

## Decision

The sessions a session messaged or heard from are read from its transcript
by name, and matched by name to the sessions running.

## Alternatives considered

- Have adroit record the messages as they pass. They pass between Claude
  Code's own sessions, not through adroit, and adroit keeps no records of its
  own about sessions.
- Ask each running session whom it messaged. Claude Code answers no such
  question, and a session that ended could not be asked.

## Consequences

- A session renamed after the messages, or one sharing its name with
  another, is matched to the session that now goes by that name.
- A session that has ended is still listed by name, though it cannot be
  shown.
- Messages to the agents inside a session are told apart from those to
  other sessions by the result Claude Code writes for them, which it does not
  promise.
