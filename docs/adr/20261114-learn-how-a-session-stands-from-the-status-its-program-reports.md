# Learn how a session stands from the status its program reports

Status: Accepted

## Context

The window has to know, per session and for any agent, whether its program is
working, waiting on the user or done, also while no window is attached.
Programs report this themselves with the OSC 7501 program status protocol,
Claude Code, Pi and OpenCode among them, and the felis daemon keeps each
session's reports. A report stands until the program sends the next one.

A report tells less than the screen at times. One of working says nothing of a
request the agent shows the user. One that the turn ended says nothing of
subagents or shells the agent left running, whose end starts a new turn.

## Decision

How a session stands is the program status the felis daemon holds for it, as
its program reports it. Adroit adds no reports of its own. The screen and the
transcript win over a report only where they show what the report cannot say:
a dialog asking the user wins over a report that it is working, and work still
running behind it wins over a report that its turn ended, showing the session
in the background.

## Alternatives considered

- Report Claude Code's state from hooks Adroit installs. They would be Adroit's
  guess at what Claude Code says of itself, written into a terminal Claude Code
  draws on.
- Read the state from Claude Code's list of sessions. It reports little, and
  nothing for other agents.
- Have Adroit keep the state. A change while Adroit is closed would be lost.
- Hold a report as true until the next one, whatever the screen shows. A
  working report the agent never followed up would hide that it waits on the
  user, and a session left running work would show as done.
- Learn the work running behind a session from its screen alone. Claude Code
  counts the subagents there with those that finished.
- Learn it from its transcript alone. It tells when each subagent ends, but
  not each shell.

## Consequences

- An agent shows its state once it reports OSC 7501; for one that reports
  nothing, its screen is all there is to read.
- Any program in a session or a terminal can report its state.
- The screen of a session reporting working, done or idle is still read, and
  the transcript of one reporting done or idle is followed.
- The terminal has to answer a program's query for the protocol, which felis
  does.
