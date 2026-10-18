# Learn how a session stands from the status its program reports

Status: Accepted

## Context

The window lists sessions waiting on the user first, and the console passes
every key to a session only while it waits on an answer. Both need to know,
per session, whether its program is working, waiting on a permission or a
question, or done. The felis daemon keeps, per session, the reports a program
makes with the OSC 7501 program status protocol, whether a window is attached
or not. Claude Code does not make those reports, but runs hooks of the
caller's at each of those moments, and tells which kind of wait it is.

## Decision

How a session stands is the program status the felis daemon holds for it.
adroit starts Claude Code with hooks that report each change of its state in
OSC 7501.

## Alternatives considered

- Read the state from Claude Code's own list of sessions. It reports only
  busy or idle for a session not in its background, and nothing for other
  agents.
- Take the state from the terminal notifications a program sends. They carry
  only a message to show, so a permission and a question could be told apart
  only by their wording.
- Have the hooks tell adroit directly. A change made while adroit is closed
  would be lost, and the state would be adroit's record rather than the
  session's.

## Consequences

- Another agent shows its state once it reports in OSC 7501, by itself or
  through hooks of its own.
- A session started outside adroit, or whose program reports nothing, shows
  no state.
