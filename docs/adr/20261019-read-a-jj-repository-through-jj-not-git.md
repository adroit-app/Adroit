# Read a jj repository through jj, not git

Status: Accepted

## Context

To git, a jj repository colocated with it looks wrong: the working-copy change
shows as uncommitted work, and git knows nothing of change IDs, empty or
conflicted changes, or bookmarks. Someone working with jj reads none of what
they work with.

## Decision

A worktree inside a jj repository is read by running the `jj` command, letting
it record the working copy as it reads, and through git only when jj cannot be
run.

## Alternatives considered

- Keep reading through git. It is what misleads.
- Link jj's library. A heavy dependency whose API follows jj's internals, and
  it can disagree with the jj the user runs about format and configuration.
- Read without recording the working copy. It would show what the worktree held
  at the last jj command, not now.

## Consequences

- jj must be on the `PATH` adroit runs with.
- A read may add an operation to the jj log.
- jj's templates and output become an interface adroit depends on.
