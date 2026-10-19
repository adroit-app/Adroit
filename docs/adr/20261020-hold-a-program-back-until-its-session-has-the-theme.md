# Hold a program back until its session has the theme

Status: Accepted

## Context

A program that picks its look by the terminal's background asks for it as it
starts, as Neovim does. felis answers from the colours a window attached to the
session has told it, and has no colours for the daemon as a whole. It starts a
session's program the moment it makes the session, and a window can tell the
colours only after that, so a program asking at once raced the window and was
often told felis's light defaults under a dark theme.

## Decision

adroit makes a session together with the window that shows it, and starts the
program behind a shell that waits for a line. The window tells the session the
theme's colours and then sends the line, so the program runs only once the
colours are there.

## Alternatives considered

- Make the session with its window and tell the colours at once, with no
  wait. A fast program can still ask first; it only makes the race rarer.
- Have adroit's daemon keep a window of its own on every session, telling the
  colours. It still attaches after the program starts, and the daemon would
  have to learn the theme from adroit.
- Give felis colours for the whole daemon, set before any session. That is
  felis's to add; adroit can take it when it lands, and drop the wait.

## Consequences

- The colours stay the window's: a program asking while no adroit window shows
  its session is told felis's defaults, as before.
- A session that adroit's command starts from another session has no window
  and is not held; its program may still be told the defaults.
- Every program adroit starts in a console runs under a POSIX shell first, and
  a program that cannot be found is caught by adroit before the session is
  made.
