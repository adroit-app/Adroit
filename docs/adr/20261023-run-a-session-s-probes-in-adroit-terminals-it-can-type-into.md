# Run a session's probes in Adroit terminals it can type into

Status: Accepted

## Context

A session that needs to try a program interactively, such as pressing a key
in Claude Code, could start an Adroit terminal and read its screen, but not
type into it, and a terminal ran only a shell or Neovim. Sessions reached for
tmux instead. Its sessions are hidden from the user, and its processes showed
up in the tree as sessions of unknown origin.

## Decision

Probes and helper programs a session starts run in Adroit terminals. A
terminal may run any program, and the `adroit` command types text, pastes and
presses keys into it, each key encoded as the window encodes it. Sessions are
told to start such programs this way and never in tmux or a hidden process.

## Alternatives considered

- Leave probes to tmux or another multiplexer of the session's choice. The
  user cannot see or step into them, and Adroit cannot tell them from sessions
  the user started elsewhere.
- Offer the same operations only as tools on Adroit's MCP server. Every
  agent can run a command, not every agent loads the server, and the server
  would grow a second way to do what the command already does.

## Consequences

- A terminal is still not an agent session: it neither starts nor messages
  sessions, whatever program it runs.
- The user sees every probe in the tree and can open, type into and stop it.
