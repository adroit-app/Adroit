# Keep request ownership independent of working directories

Status: Accepted

## Context

Independent requests can run in the same repository, while workers handling
one request can run in different worktrees. A directory describes where an
agent works, but does not identify the request it handles. A terminal's ID
and a provider's conversation ID each have their own lifetime.

## Decision

Keep request ownership independent of working directories. An assigned agent
has an explicit logical identity and a leader or worker role. A worker belongs
to a particular leader, request, task and execution attempt. The workspace
and ownership projections refer to the same sessions.

This extends the directory grouping chosen in
[Find sessions by the directory they run in, keeping no project file](20261018-find-sessions-by-the-directory-they-run-in-keeping-no-project-file.md).

## Alternatives considered

- Use the repository or worktree as the request. Independent leaders in the
  same repository would share workers, and one request could not span
  worktrees.
- Infer ownership from names or provider conversation ancestry. Names can
  change, and delegation can cross providers.
- Put another leader above independent requests. It would join unrelated
  contexts without resolving the distinction between location and ownership.

## Consequences

- Logical identities do not replace terminal or conversation identities.
- An agent without an assignment remains unassigned; its directory does not
  supply an owner.
- A missing leader or mismatched request leaves a worker reachable without
  attaching it to another request.
