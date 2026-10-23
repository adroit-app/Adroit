# Split adroit into a terminal host, a work host that owns the work, and a window that only shows it

Status: Proposed

## Context

The session daemon holds the sessions' terminals, and also runs delegation and
judges whether a session is ready. Both change often, and a fix to either
reached nothing until the daemon was replaced, which ends every session. The
window, for its part, keeps what is known of the work in its own memory and
reads files, repositories and processes itself, so it can only run on the
machine the sessions run on. The window is meant to run, in time, on another
machine than the sessions.

## Decision

adroit runs as three parts.

- The terminal host holds each session's pseudo-terminal and emulator, and
  nothing else.
- The work host owns everything known of the work: which sessions there are
  and where, who started and messaged whom, how each session stands, and the
  files, repositories and programs on its machine. Every program a session or
  the window needs runs here, the Neovims the window draws among them.
- The window holds only how the work is shown. It draws what the hosts send
  and passes on what the user does, and it reaches the hosts' machine through
  their sockets alone.

Both hosts run on the machine the sessions run on. The window sends keys to
and draws screens from the terminal host directly.

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
- The window never opens a file, runs a command or reads a repository itself;
  the tree, the viewers and the details ask the work host.
- Neovim runs on the hosts' machine, and the window drives it as a remote
  interface.
- A place is named by its host as well as its path.
- With nothing left for the window to answer, nothing keeps an instance to one
  window.
- What a program on the hosts' machine copies reaches the window only through
  its terminal.
