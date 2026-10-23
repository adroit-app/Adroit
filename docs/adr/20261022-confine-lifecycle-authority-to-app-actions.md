# Confine lifecycle authority to app actions

Status: Superseded by 20261023-give-every-session-the-same-means-to-delegate.md

## Context

Managed Agents run under the same user as Adroit and can read the human socket
credential. Refusing one authorization command is insufficient when lifecycle
commands can be composed to register, grant, and launch an independent Leader.
The CLI can also be invoked with Agent environment markers removed.

## Decision

The human socket credential grants observation only. App initiated lifecycle
changes use a separate in-process path; managed Agents use their scoped session
credentials. Independent request authorization remains an explicit window action.
The CLI refuses automatic managed Leader starts and directs the user to the
window; its unassigned session starts remain available.

## Alternatives considered

- Deny only the known composition. Other lifecycle commands could still mint or
  redirect authority.
- Treat the human credential as proof of a window action. Agents can read it.

## Consequences

- Socket clients cannot create or prepare an independent Leader using the human
  credential, including through combinations of supported commands.
- The app can still create and manage sessions, and authorized Leaders can
  redeem their grants.
- A CLI request that would infer a managed Leader requires a window action.
- This is a supported API boundary, not isolation from arbitrary same-user
  filesystem writes or code execution.
- Workspace path identity checks reduce accidental replacement but cannot
  eliminate a pathname swap between check and native launch.
