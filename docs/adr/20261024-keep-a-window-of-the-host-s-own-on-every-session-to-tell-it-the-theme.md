# Keep a window of the host's own on every session to tell it the theme

Status: Accepted

## Context

A program that picks its look by the terminal's background asks for it,
Neovim as it starts and Codex once it has started up. felis answers from the
colours the windows attached to the session have told it, and with no window
attached, with xterm's black on white. felis also starts the program the
moment it makes the session. A program raced the window that showed it, and
one whose session no window showed, such as one a session started, was told
a white background and drew its surfaces for a light terminal on a dark one.

## Decision

The work host keeps a window of its own on every session and shell it starts,
from before the program runs until the session ends. That window shows
nothing: it tells the session the theme's colours, and the new ones whenever
the theme changes, and sends no input or size. The program starts behind a
shell that waits for a line, sent once the host's window is attached.

## Alternatives considered

- Keep the window that started the session attached. It sent the line that
  let the program go, which made it the session's owner of size, so a window
  showing the session later could not size it until typed into.
- Wait a while before letting the starting window go. A program may ask at
  any time, and how long one takes to start is not known.
- Have the program set its colours itself before it runs. Colours a program
  sets win over the theme, so a new theme would no longer reach it.
- Colours for the whole felis daemon, set before any session. That is felis's
  to add; Adroit can drop its own window when it lands.

## Consequences

- A program is told the theme's colours whenever it asks, whether a window
  shows its session or not.
- The host holds a connection to the terminal host for every session it
  runs, which carries next to nothing as it asks for no screen.
- Sessions started before the host last started have no such window until
  they are started again.
- Every program Adroit starts in a console runs under a POSIX shell first.
