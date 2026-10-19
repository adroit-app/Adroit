# Read a jj repository through jj, not git

Status: Accepted

## Context

The details panel read every repository through git. A jj repository
colocated with git looks wrong to git: HEAD is the parent of the working-copy
change, so that change shows as uncommitted work, and git knows nothing of the
stack's change IDs, of which changes are empty or conflicted, or of
bookmarks. Someone working with jj reads none of what they work with.

## Decision

A worktree inside a jj repository is read by running the `jj` command, letting
it record the working copy as it reads, and through git only when jj cannot be
run.

## Alternatives considered

- Keep reading through git. It is what misleads.
- Link jj-lib and read the repository in process. It is a heavy dependency
  whose API follows jj's internals from release to release, and a library
  built into adroit can disagree with the jj the user runs about the
  repository's format and their configuration.
- Read with jj's `--ignore-working-copy`, so that reading never records the
  working copy. What `@` changes would be what it held at the last jj command
  someone ran, not what the worktree holds now.

## Consequences

- jj must be on the `PATH` adroit runs with.
- Every read is a jj command, which records the working copy as any jj
  command does, and may add an operation to the log.
- jj's template language and the output it reads become an interface adroit
  depends on.
