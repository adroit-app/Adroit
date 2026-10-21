# Require human authorization for a managed Leader to create an independent Leader

Status: Superseded by 20261022-use-window-action-for-independent-authorization.md

## Context

Leaders need to hand unrelated requests to independent Leaders in specified
workspaces. A general session creation tool would let an Agent bypass the
human's choice of workspace and request boundary.

## Decision

A human authorizes the exact independent request and launch selection for a
specific managed Leader. That Leader can redeem the durable authorization once;
the resulting run and Leader have new identities and independent ownership. The
general Leader registration route remains human only.

## Alternatives considered

- Let Leaders register arbitrary sessions. This grants more authority than the
  human authorized and lets Workers attempt the same operation.
- Infer an independent run from a directory or conversation. Neither identifies
  the user's intended request boundary.

## Consequences

- A new Leader is not a subagent of the requesting Leader.
- A retry can recover the same identities without creating another run.
- The human must specify the request before the Leader can hand it off.
