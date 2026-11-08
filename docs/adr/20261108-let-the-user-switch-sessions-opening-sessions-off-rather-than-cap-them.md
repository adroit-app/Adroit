# Let the user switch sessions opening sessions off rather than cap them

Status: Accepted

## Context

How many sessions opened by sessions could run at once was capped. The cap
dates from when sessions started workers that did parts of their work; no
session waits on another's result now, so nothing has to bound how far
sessions open sessions. The cap counted every such session, in every
repository and whoever opened it, so sessions in unrelated work used up the
room and a session the user had asked to open several sessions was refused.
The one thing the cap also did that still matters: when the settings could not
be read, a cap of zero kept sessions from opening sessions rather than handing
out room the user may have taken away.

## Decision

Sessions opening sessions is switched on or off, and nothing counts them. It is
on unless the user switches it off, and off while the settings cannot be read.
Off refuses a start, telling the agent to carry on itself or ask the user, and
leaves running sessions alone. A cap written in an earlier settings file is
read and dropped.

## Alternatives considered

- Count per repository, or per opening session. A session asked to open
  several sessions is still refused at an arbitrary number, and the number
  still has no reason behind it.
- Keep the cap with a higher default. The user had already raised it by hand
  and was still refused; a number that never refuses is a switch.
- Turn an earlier cap of zero into off. It keeps one pause through the change,
  at the cost of a setting read with two meanings; dropping it keeps the file
  meaning one thing.

## Consequences

- Nothing bounds how many sessions opened by sessions run; the user ends the
  ones not needed.
- A start is refused for being switched off before anything is made for it,
  so a refused start leaves no worktree behind.
- A settings file that capped sessions at zero opens sessions again once read.
