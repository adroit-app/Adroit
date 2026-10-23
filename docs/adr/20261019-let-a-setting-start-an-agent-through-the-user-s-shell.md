# Let a setting start an agent through the user's shell

Status: Accepted

## Context

Many users set what their tools need, keys and version managers among them, in
an interactive shell's startup files, which a program started directly never
reads. Others want a session to start the same way whatever their shell does.

## Decision

Whether an agent starts directly or through the user's shell is a setting,
direct by default. Through the shell, adroit starts an interactive login shell
that `exec`s the agent, handing the program and its arguments over as the
shell's arguments, never written into a script.

## Alternatives considered

- Always start through the shell. A shell's startup can print, prompt or run
  something else, and slows every session.
- Read the shell's variables once and reuse them. A shell sets more than
  variables, and what it sets can change while adroit runs.
- Write the command into a script for the shell. Every word needs quoting per
  shell, and a mistake runs something else.

## Consequences

- Only shells that take their command's arguments, POSIX shells and fish, can
  start an agent.
- What a session runs depends on the user's settings, so a failing session's
  report needs them.
