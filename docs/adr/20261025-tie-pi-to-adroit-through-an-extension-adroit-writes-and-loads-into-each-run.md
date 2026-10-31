# Tie Pi to Adroit through an extension Adroit writes and loads into each run

Status: Accepted

## Context

Every session Adroit starts carries Adroit's MCP server and hears the
messages other sessions send it through its agent's own way of hearing from
outside. Claude Code and Codex take a server for one run on their command
line and have such a way. Pi has neither: it reads MCP servers only from its
configuration, and has no channel or queue a program outside it can put a
message on. It runs extensions, though, loaded for one run from a path on its
command line, which can register a server, put a message to the agent as the
user's and see which conversation the process holds.

## Decision

Adroit writes an extension of its own into its state folder and loads it
into every Pi session it starts. The extension registers Adroit's MCP server
for a session that carries Adroit's credential, hears that session's
messages from Adroit's server as Claude Code does, and hands each to Pi as a
message from the user, and keeps a record of the conversation the process
holds for Adroit to follow it.

## Alternatives considered

- Write Adroit's server into the configuration of the profile's own Pi.
  Adroit would put its own files into a state folder it otherwise only names,
  and every Pi run in that folder, Adroit's or not, would carry the server.
- Type each message into the session's console. It would land in whatever
  the user is typing, and a dialog open would take it as an answer.
- Leave Pi without Adroit's tools and messages. A Pi session could neither
  start nor message sessions, nor hear its report, against the decision that
  every session Adroit starts has the same means.

## Consequences

- A Pi session hears its messages only while the extension runs: a Pi that
  loads no extensions, or one Adroit did not start, hears none.
- Adroit's server takes a message as received once the extension has heard
  it, as it does once Claude Code has. The extension keeps one heard while
  no conversation is open for the next, so a Pi that ends before then loses
  it, and it is not sent again.
- The extension runs inside Pi with the user's rights, so what Adroit writes
  there is code it ships, written afresh before each session.
- The extension follows Pi's extension interface, which is unversioned; a
  release that changes it leaves a Pi session without Adroit until Adroit
  follows.
