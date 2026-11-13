# Bind keys in the console of each agent before the console's own

Status: Accepted

## Context

The console's bindings are the same for every session, but the agents behind
them are not. A key such as Tab does one thing in Claude Code's input box and
another, or nothing, in the others', and the prompt Adroit puts beside a
session may already do what the user means by it. A binding that suits one
agent was either wrong for the rest or missing for it.

## Decision

Each agent has an area of bindings of its own for its sessions' console, live
or read, looked up before the console's. Adroit ships bindings there as it
does in any area, and the user's file changes, adds to or takes them away in
the same way.

## Alternatives considered

- Make the console's bindings themselves depend on the agent in the code. The
  user could neither see nor change what differs.
- Let a profile carry bindings. Sessions of the same agent would answer one
  key differently by the profile they started from, and the keys file would
  no longer hold every binding.

## Consequences

- A binding meant for every agent goes in the console's area; one in an
  agent's area hides it for that agent alone.
