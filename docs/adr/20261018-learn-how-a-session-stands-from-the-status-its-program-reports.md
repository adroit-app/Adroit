# Learn how a session stands from the status its program reports

Status: Accepted

## Context

The window lists sessions waiting on the user first, and passes every key to
a session while it waits on an answer. Both need to know, per session,
whether its program is working, waiting on a permission or a question, or
done, whether a window is attached or not, and for any agent, not only Claude
Code. Programs report this themselves with the OSC 7501 program status
protocol, Claude Code among them, and the felis daemon keeps each session's
reports while no window is attached.

## Decision

How a session stands is the program status the felis daemon holds for it, as
its program reports it. adroit adds no reports of its own.

## Alternatives considered

- Report Claude Code's state from hooks adroit installs. The reports would be
  adroit's guess at what Claude Code says of itself, written into a terminal
  Claude Code is drawing on at the same moment.
- Read the state from Claude Code's own list of sessions. It reports only
  busy or idle for a session not in its background, and nothing for other
  agents.
- Have adroit keep the state, told by the program some other way. A change
  made while adroit is closed would be lost, and the state would be adroit's
  record rather than the session's.

## Consequences

- Another agent shows its state once it reports OSC 7501.
- A program that reports nothing shows as idle, and a Claude Code too old to
  report shows so too.
- The terminal the session runs in has to answer a program's query for the
  protocol, which felis does.
