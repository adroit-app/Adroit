# Keep each agent's settings together

Status: Accepted

## Context

Adroit runs more than one agent. Splitting their launch options from their
session options makes each new agent add forms in several places. Agents
also differ in how they control execution and permissions.

## Decision

Keep each agent's settings together, with separate defaults for sessions
and readers. Share navigation and launch controls while letting each agent
expose its own execution options.

## Alternatives considered

- Keep adding agents to the shared session and program forms. Finding one
  agent's settings would require searching increasingly long forms.
- Give every agent identical execution controls. Different permission and
  sandbox models would lose meaning in the translation.

## Consequences

- Adding an agent extends the agent list and its settings without expanding
  every agent's form.
- Each agent retains responsibility for translating its settings into its
  own launch arguments.
