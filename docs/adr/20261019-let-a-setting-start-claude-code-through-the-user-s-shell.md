# Let a setting start Claude Code through the user's shell

Status: Accepted

## Context

adroit started Claude Code directly, with its own environment and the `PATH`
the login shell gives. Many users set what their tools need, API keys, version
managers and the like, in the startup files of an interactive shell, which a
program started directly never reads. Others want the opposite: a session that
starts the same way whatever their shell does on startup.

## Decision

Whether Claude Code starts directly or through the user's shell is a setting,
direct by default. Through the shell, adroit starts the shell as an interactive
login shell and has it `exec` Claude Code, handing the program and its
arguments over as the shell's own arguments rather than writing them into a
script. Either way the settings may add variables.

## Alternatives considered

- Always start through the shell. A shell's startup can print, prompt or
  start something else, and slows every session down; a user who sets nothing
  there pays for it.
- Read the variables a shell sets once and start every session with them. A
  shell sets more than variables, functions and hooks among them, and what it
  sets can change while adroit runs.
- Write the command into the shell's script. Every word would need quoting for
  each shell, and a mistake runs something else.

## Consequences

- Through the shell works for a shell that takes its command's arguments, a
  POSIX shell or fish; another shell needs a direct start.
- What a session runs differs from one user's settings to another's, so a
  report of a failing session needs those settings.
