# Bind work to environment profiles

Status: Accepted

## Context

Choosing a work machine independently of its agents and settings leaves users
managing internal client instances and remembering which configuration belongs
where. Two environments on the same machine also need different agents and
working directories without requiring separately managed services.

## Decision

A named environment profile binds a connection to a work environment. Agents,
work settings and running work belong to that environment. Client instances
keep their binding while running; opening another profile opens its environment
without moving or redirecting existing work. Display and input preferences stay
on the client computer.

## Alternatives considered

- Keep the connection separate from agent configuration. This makes users
  reconstruct the environment themselves and allows settings from another
  machine to be mistaken for the selected machine's capabilities.
- Give every profile independently managed host services. This makes adding an
  environment a service administration task.
- Redirect a running instance when a profile changes. This changes the meaning
  of existing child commands, drafts and unresolved operations.

## Consequences

The work host serves separate environments over its shared services. Each
connection selects its environment before requesting work. Agent launch presets
remain inside that environment, and their credentials remain on its work
machine. Existing environments need an identity-preserving migration; a profile
name alone cannot identify work or authorize a reconnection.
