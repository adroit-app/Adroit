# Tie OpenCode to adroit through a plugin adroit writes and loads into each run

Status: Accepted

## Context

Every session adroit starts carries adroit's MCP server and hears the
messages other sessions send it through its agent's own way of hearing from
outside. OpenCode takes an MCP server for one run through its configuration,
but has no channel or queue a program outside it can put a message on, and
the server of an interface's own cannot be reached from outside it. It runs
plugins in that server, loaded for one run from its configuration, which can
follow the conversation the interface shows, put a message to it as the
user's after the turn running, and change what the commands a session runs
are given.

## Decision

adroit writes a plugin of its own into its state folder and loads it into
every OpenCode session it starts. The plugin hears the session's messages from
adroit's server, as Claude Code does, and queues each to the conversation the
interface shows once that conversation exists.

## Alternatives considered

- Run a server of adroit's own for each session and send through OpenCode's
  command that sends a message to a conversation. That command answers every
  permission the conversation asks while it runs, refusing it unless told to
  approve everything, so it would answer for the user.
- Type each message into the session's console. It would land in whatever
  the user is typing, and a dialog open would take it as an answer.
- Leave OpenCode without messages. An OpenCode session could not hear a
  report or a message, against the decision that every session adroit starts
  has the same means.

## Consequences

- An OpenCode session hears its messages only while the plugin runs: an
  OpenCode that loads no plugins, or one adroit did not start, hears none.
- adroit's server takes a message as received once the plugin has heard it,
  so a message still waiting in the plugin is lost if OpenCode ends first:
  every message heard before the conversation is made, which happens when its
  first prompt is sent, waits there.
- The plugin runs inside OpenCode with the user's rights, so what adroit
  writes there is code it ships, written afresh before each session.
- The plugin follows OpenCode's plugin interface, which is young; a release
  that changes it leaves an OpenCode session without adroit's messages until
  adroit follows.
