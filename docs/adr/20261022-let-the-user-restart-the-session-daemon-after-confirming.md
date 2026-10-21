# Let the user restart the session daemon after confirming

Status: Accepted

## Context

The daemon outlives the window, so after adroit is updated the window keeps
talking to a daemon of the build before. Until now the only way to replace it
was to find its process and kill it by hand. Replacing it ends every session
it holds, which is why
`20261021-read-ui-state-without-restarting-the-session-daemon.md` keeps
the window from restarting it on its own.

## Decision

adroit stops and restarts its daemon only when the user asks: from the
command line, or from the window after confirming that every live session
ends. A window that finds the daemon is of another build, or too old to say
which, offers that restart and does nothing more.

Stopping asks the daemon to end its sessions and exit, and signals its
process when the daemon cannot be asked, so a daemon from before stopping
was possible is still replaced.

## Alternatives considered

- Restart a daemon of another build as soon as the window finds it. This
  ends live sessions without the user choosing to.
- Upgrade the daemon in place, keeping its sessions. adroit's daemon cannot
  resume sessions handed over to its successor, and a daemon already running
  an old build cannot hand them over.
- Leave replacing the daemon to the user's own tools. They have to find its
  process, and nothing tells them that it is out of date.

## Consequences

- Restarting ends every session the daemon holds: agents, shells and what
  they scrolled back.
- The daemon has to say which build it is, for the window to tell an old one.
- Upgrading in place remains open once the daemon can resume sessions.
