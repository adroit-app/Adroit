# Hold a program back until its session has the theme

Status: Accepted

## Context

A program that picks its look by the terminal's background asks for it as it
starts, as Neovim does. felis answers from the colours a window attached to the
session has told it, and starts the program the moment it makes the session, so
the program raced the window and was often told the wrong background.

## Decision

A session's program starts behind a shell that waits for a line. The window
that shows the session attaches, tells it the theme's colours, and only then
has the line sent. A session no window comes to is let go after a short wait.

## Alternatives considered

- Tell the colours right after making the session. A fast program still asks
  first.
- Keep a host-side window on every session that tells the colours. It still
  attaches after the program starts.
- Colours for the whole felis daemon, set before any session. That is felis's
  to add; adroit can drop the wait when it lands.

## Consequences

- A program whose session no window shows as it starts, such as one a session
  started, is told felis's defaults.
- Every program adroit starts in a console runs under a POSIX shell first.
