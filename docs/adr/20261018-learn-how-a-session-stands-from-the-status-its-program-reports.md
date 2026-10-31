# Learn how a session stands from the status its program reports

Status: Accepted

## Context

The window has to know, per session and for any agent, whether its program is
working, waiting on the user or done, also while no window is attached.
Programs report this themselves with the OSC 7501 program status protocol,
Claude Code among them, and the felis daemon keeps each session's reports.

## Decision

How a session stands is the program status the felis daemon holds for it, as
its program reports it. Adroit adds no reports of its own.

## Alternatives considered

- Report Claude Code's state from hooks Adroit installs. They would be Adroit's
  guess at what Claude Code says of itself, written into a terminal Claude Code
  draws on.
- Read the state from Claude Code's list of sessions. It reports little, and
  nothing for other agents.
- Have Adroit keep the state. A change while Adroit is closed would be lost.

## Consequences

- An agent shows its state once it reports OSC 7501; one that reports nothing
  shows as idle.
- The terminal has to answer a program's query for the protocol, which felis
  does.
