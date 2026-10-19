# Ask Claude Code for the usage limits it reports

Status: Accepted

## Context

The overview tells how much of the account's usage limits is left, as Claude
Code's `/usage` does. The limits belong to the account, not to a session:
which ones exist differs by plan and changes as Anthropic changes them, and
only Anthropic's servers know how much of each is used.

## Decision

The overview shows the limits that Claude Code itself reports for `/usage`,
asked of a `claude` process started only to answer, with no message sent, as
the slash commands are asked for in
`20261019-ask-claude-code-for-its-slash-commands.md`.

## Alternatives considered

- Read the rate limits Claude Code hands a status line command. adroit would
  have to install its own status line into every session, displacing the
  user's, and the figures arrive only after a session's first reply and only
  for the session and weekly windows.
- Call the usage endpoint Claude Code calls, with the credentials it keeps.
  adroit would read Claude Code's secrets and depend on an undocumented
  server API, both of which Claude Code is free to change.
- Read `/usage` off a session's screen. It needs a session to type into, and
  the drawing is not meant to be parsed.

## Consequences

- The request is internal to Claude Code, marked by it as experimental; a
  release that changes it leaves the overview without limits until adroit
  follows.
- Each ask starts a short-lived `claude` process that asks Anthropic's
  servers; it runs no turn and spends no tokens.
- Limits Claude Code adds appear without a change to adroit, under Claude
  Code's name for them.
