# Ask Codex for its enabled skills

Status: Accepted

## Decision

Ask Codex for the current directory's enabled skills, and offer them beside
the built-in commands catalog. Selecting a skill inserts the invocation that
the Codex TUI understands.

## Alternatives considered

- Read skill folders directly. Codex already resolves user, project and
  plugin skills and their enabled state.
- Use an ACP adapter for discovery. It adds another runtime and translates
  commands for its own session instead of the TUI that Adroit runs.
- Keep only the built-in catalog. It leaves the user's skills undiscoverable
  in the embedded prompt.

## Consequences

Built-in commands are available while skill discovery is pending or fails.
Skill discovery starts no thread or turn, and follows Codex's configuration.

## Context

The built-in catalog chosen in
`20261020-keep-a-catalog-of-codex-built-in-slash-commands.md` covers the
TUI commands. Codex separately exposes enabled skills through its app-server,
so those can be discovered without maintaining another catalog.
