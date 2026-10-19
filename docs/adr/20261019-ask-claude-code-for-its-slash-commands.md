# Ask Claude Code for its slash commands

Status: Accepted

## Context

The prompt offers commands after `/`, as Claude Code's own input does. Which
commands exist differs by account and by release: built-in ones come and go,
and the account's skills, plugins and MCP servers add their own, some only in
some directories.

## Decision

The prompt offers the commands that Claude Code itself lists for the directory,
asked of a `claude` process started only to answer, with no message sent.

## Alternatives considered

- Keep a table of commands in adroit. It drifts with every release and knows
  none of the account's own.
- Read the command and skill folders under `~/.claude` and the project. It
  misses built-in, plugin and MCP commands.
- Ask through the Agent SDK or an ACP adapter. Either adds a runtime that adroit
  would have to find or install beside Claude Code.

## Consequences

- The request is internal to Claude Code and unversioned; a release that changes
  it leaves `/` offering nothing until adroit follows.
- Completing starts a short-lived `claude` process for a directory, again each
  time its list has aged; it runs no turn and spends no tokens.
