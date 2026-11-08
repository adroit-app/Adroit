# Learn how a session stands from the status its program reports

Status: Accepted

## Context

The window has to know, per session and for any agent, whether its program is
working, waiting on the user or done, also while no window is attached.
Programs report this themselves with the OSC 7501 program status protocol,
Claude Code, Pi and OpenCode among them, and the felis daemon keeps each
session's reports. A report stands until the program sends the next one.

## Decision

How a session stands is the program status the felis daemon holds for it, as
its program reports it. Adroit adds no reports of its own. A dialog the agent
shows on screen asking the user wins over a report that it is working.

## Alternatives considered

- Report Claude Code's state from hooks Adroit installs. They would be Adroit's
  guess at what Claude Code says of itself, written into a terminal Claude Code
  draws on.
- Read the state from Claude Code's list of sessions. It reports little, and
  nothing for other agents.
- Have Adroit keep the state. A change while Adroit is closed would be lost.
- Hold a report as true until the next one, whatever the screen shows. A
  working report the agent never followed up would hide that it waits on the
  user.

## Consequences

- An agent shows its state once it reports OSC 7501; one that reports nothing
  shows as idle.
- Any program in a session or a terminal can report its state.
- The screen of a session reporting working is still read for dialogs.
- The terminal has to answer a program's query for the protocol, which felis
  does.
