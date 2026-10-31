# Bind work to named environments

Status: Accepted

## Context

Choosing a work machine independently of its agents and settings left users
managing internal client instances and remembering which configuration belongs
where. Two environments on the same machine also need different agents and
working directories without separately managed services. The word profile
already names an agent's launch setup, which lives inside an environment.

## Decision

A named environment binds a connection to its work: its agents, work settings
and running work. A connection keeps its environment while it is open;
another environment is reached by a connection of its own beside the first,
without moving or redirecting existing work. Display and input preferences
stay on the client computer.

## Alternatives considered

- Keep the connection separate from agent configuration. Users reconstruct the
  environment themselves, and settings from another machine can be mistaken
  for the selected machine's capabilities.
- Give every environment independently managed host services. Adding an
  environment becomes a service administration task.
- Redirect an open connection when its environment changes. The meaning of
  existing child commands, drafts and unresolved operations changes under them.
- Call an environment a profile. One word would name both an environment and
  the agent setups inside it.

## Consequences

- The work host serves separate environments over its shared services, and
  each connection selects its environment before requesting work.
- Agent profiles remain inside an environment, and their credentials remain on
  its work machine.
- Existing installations need an identity-preserving migration; an
  environment's name alone cannot identify work or authorize a reconnection.
