# Let a file's row stand for its Neovim, and quit it as `:qa` does once no window shows it

Status: Accepted

## Context

A file opened from the tree that has no viewer opens in a Neovim started on
it. Listed as a terminal of its own, that Neovim stood apart from the file it
edits, and every file once opened left one running, hidden, until the user
went looking for it to end it. A viewer, the other way a file is opened, ends
with the last window showing it and loses nothing by it. A Neovim can lose
work: a buffer with a change not yet written.

## Decision

A Neovim started on a file has no row of its own: the file's row stands for
it, marked while it runs. Once no window of the instance shows it any longer,
the work host, which knows what every window shows, asks it to quit as `:qa`
does, typing the command into it through the terminal host. Neovim itself
decides: it refuses while a buffer holds an unsaved change, and then runs on,
reached again through the file's row.

## Alternatives considered

- Keep listing it as a terminal, under a heading of its own. The same work
  stands in two places, and nothing ends what is no longer looked at.
- End it outright, as a viewer ends. An unsaved change would be lost.
- Ask it to quit through an RPC socket it listens on. Neovims already running,
  and those a configured command starts without the socket, could not be
  reached; typing reaches every one.
- Let each window decide from what it shows alone. A window would end a
  Neovim another window of the instance still shows.

## Consequences

- What is typed goes through Neovim's own keys: a user mapping on `:` or the
  keys that leave a mode can keep it from quitting, and then it runs on.
- Closing Adroit's window itself ends none, as it ends no other program.
- A Neovim started on no file is a terminal like a shell, ended by the user.
