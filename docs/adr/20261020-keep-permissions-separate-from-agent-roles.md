# Keep permissions separate from agent roles

Status: Accepted

## Context

A reader is an ordinary agent used for investigation or review. Its separate
settings and draft identity couple permissions to a session kind, while a
leader or worker may need either read-only or editing access.

## Decision

Keep permissions separate from agent roles. An ordinary agent launch chooses
its permission profile independently of its provider, model and assignment.
Remove the separate reader session kind and its configuration and commands.

## Alternatives considered

- Keep readers as read-only workers. This would still duplicate the same
  agent configuration and confuse a task's role with its access rights.
- Treat plan mode as read-only enforcement. Conversation mode does not by
  itself prevent writes through all available tools.

## Consequences

- A permission or model change does not replace a draft or its prompt.
- Providers can support different profiles; an unsupported request fails
  explicitly rather than silently granting different permissions.
- Embedded Neovim for reading terminal output remains a UI component.
