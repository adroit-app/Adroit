# Group what runs in a worktree under files, sessions, shells and editors

Status: Accepted

## Context

A worktree listed its sessions and its shells mixed in one list, a shell
marked as a terminal. Editors and the worktree's own files are now opened
from the tree too, and a long mixed list hides what kind each row is.

## Decision

Each worktree in the tree holds four headings, Files, Sessions, Shells and
Editors, and every row sits under the heading of its kind. A heading folds.
Shells and editors always sit under a worktree, never straight under a
repository.

## Alternatives considered

- Keep one list per worktree and tell the kinds apart by icon. Files alone
  would bury the sessions, and a kind could not be folded away.
- Group by kind at the top of the tree and by worktree below. The work of one
  worktree would be split across the tree.

## Consequences

- A worktree where only shells or editors run is listed for them.
- Sessions started by sessions keep being listed under their starter, now
  inside the Sessions heading.
- The file tree of a worktree lives in the same tree as its sessions.
