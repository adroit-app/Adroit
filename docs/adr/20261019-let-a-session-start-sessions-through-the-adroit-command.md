# Let a session start sessions through the adroit command

Status: Superseded for managed Leader starts by 20261022-confine-lifecycle-authority-to-app-actions.md

## Context

A Claude Code session hands work to agents of its own, which run inside it:
the user cannot read them in their own console, write to them, or take one
over. Work handed to a session of adroit's instead is a session like any
other, shown and steered as one. Claude Code already lets local sessions
message each other by name, so a session that started another can hear back
from it without adroit's help, and it tells a command it runs which
conversation it is.

## Decision

A session starts another by running `adroit session start`, which asks the
felis daemon for it directly, not the window. The new session carries in its
tags the conversation it was started from. Claude Code learns of the command
from a plugin adroit hands every session it starts. What the sessions say to
each other goes through Claude Code's own messaging; adroit carries none of
it.

## Alternatives considered

- Take over Claude Code's agents, running each as a session of adroit's
  instead. Claude Code runs them in its own process and offers no way to run
  one elsewhere.
- An MCP server of adroit's. It serves Claude Code alone and has to be
  registered with it, while a command serves any program a session runs, a
  shell included.
- Start the session through the window. A session left working while the
  window is closed could not start one.
- Have adroit pass the answer back, waiting for the session to finish and
  reading its last reply. Claude Code's messaging already does this, and lets
  the sessions talk while they work, not only at the end.

## Consequences

- A session can fill the window with sessions of its own; nothing limits how
  many.
- Which session started which is known only while the started one runs.
- Sessions adroit did not start do not have the plugin, and do not know the
  command unless told.
