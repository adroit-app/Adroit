# Run each terminal agent through its own adapter

Status: Accepted

## Context

Adroit was built around Claude Code. Codex also has an interactive terminal
interface, but its launch options and optional integrations differ. More
terminal agents may follow.

## Decision

Run each terminal agent through its own adapter. The adapter owns the
provider's launch options and integrations; the window and session command
use the same interface. A running session retains its provider independently
of the default for new sessions.

## Alternatives considered

- Replace the Claude executable with a configurable command. Claude's
  arguments and control requests would still reach an incompatible program.
- Branch on the provider wherever the window needs it. Every new provider
  would require changes throughout the application.

## Consequences

- Adding a provider requires an adapter and its registration.
- Providers share the terminal and prompt without promising identical
  optional integrations.
- Existing Claude sessions and configuration remain readable.
