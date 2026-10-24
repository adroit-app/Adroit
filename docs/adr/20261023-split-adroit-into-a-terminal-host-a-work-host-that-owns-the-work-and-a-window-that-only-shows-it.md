# Split adroit into a terminal host, a work host that owns the work, and a window that only shows it

Status: Accepted

## Context

The session daemon held the sessions' terminals, and also ran delegation and
judged whether a session was ready. Both change often, and a fix to either
reached nothing until the daemon was replaced, which ends every session. The
window, for its part, kept what was known of the work in its own memory and
read files, repositories and processes itself, so the work was gone while it
was closed, and it could only run on the machine the sessions run on. The
window is meant to run, in time, on another machine than the sessions.

## Decision

adroit runs as three kinds of process.

- The terminal host holds each session's pseudo-terminal and emulator, and
  nothing else.
- The work host owns everything known of the work: which sessions there are
  and where, who started and messaged whom, how each session stands, the
  prompts' texts, and the files, repositories, programs and settings on its
  machine. Sessions, shells and the commands run in them start here.
- The window holds only how the work is shown. It learns the work from the
  work host and changes it by asking the work host; it sends keys to and draws
  screens from the terminal host directly.

Both hosts run on the machine the sessions run on, and are reached through
their sockets alone.

## Alternatives considered

- Keep the work in the session daemon, as it was. Every change to the work
  ends the sessions to take effect.
- One host that hands its terminals to its successor when replaced. The
  emulator's state lives in felis, which cannot pass it on.
- Keep the work in the window. The work is gone while the window is closed,
  and the window cannot run apart from the sessions.
- Run the window on the hosts' machine too and send only its picture. Every
  draw and scroll pays the network's delay.
- Have the work host carry the terminals' keys and screens. Every key takes
  one more hop, and the terminals stall while the work host is replaced.

## Consequences

- The work host can be replaced at any time without ending a session; only a
  change to the terminal host's protocol ends them.
- The window reads no repository and runs no command of the work's; the tree,
  the picker and the details ask the work host. The window's code cannot reach
  the code that reads repositories or runs agents.
- A place is named by its host as well as its path.
- With nothing of the work left in the window, nothing keeps an instance to
  one window.
- What a program on the hosts' machine copies reaches the window only through
  its terminal.
