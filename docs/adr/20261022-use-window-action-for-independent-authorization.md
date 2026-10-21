# Use a window action for independent authorization until Agents are isolated

Status: Superseded by 20261022-confine-lifecycle-authority-to-app-actions.md

## Context

The daemon's human credential is a file owned by the same macOS user that runs
managed Agents. An Agent able to run ordinary shell commands can read that
credential and impersonate the human client. It can also access the local state
database. Token checks and an Agent-provided environment variable cannot prove
that a person authorized a request.

## Decision

The supported independent-request authorization entry is an explicit action in
the Adroit window. The control socket refuses authorization regardless of its
credential, and the Agent CLI requires a managed credential. A durable grant
binds one Leader generation, the exact request and launch selection, and the
workspace identity; only that Leader can redeem it once.

This is an interaction boundary against accidental or cooperative Agent use, not
an operating-system security boundary against hostile same-user code. Before
claiming that stronger boundary, Adroit must isolate Agent execution from human
authority and state through a separate OS identity, sandbox, or privileged
broker that verifies the user gesture.

## Alternatives considered

- Keep the human-token socket command and add an environment or token check. The
  Agent can unset the variable and read the same token.
- Hide the command from the CLI or MCP catalog. The socket remains callable.
- Describe same-user filesystem permissions as isolation. The Agent runs with
  the same access and can inspect the database and credential.

## Consequences

- Users authorize a specific request in the window; the Leader receives the
  grant to redeem through its scoped control tool.
- Workspace identity is checked on redemption and before launch; pathname
  replacement remains possible between check and use.
- The human-token lifecycle route was later removed by
  20261022-confine-lifecycle-authority-to-app-actions.md.
