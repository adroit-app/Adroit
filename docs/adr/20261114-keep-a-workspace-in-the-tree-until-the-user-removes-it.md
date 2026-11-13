# Keep a workspace in the tree until the user removes it

Status: Accepted

## Context

The tree listed a workspace only while something ran in it. A repository
vanished from the tree the moment its last session or shell ended, and one the
user meant to work in could be listed only by starting something there. The
tree is where the user goes back to their work, so it lost the places they
return to most.

## Decision

The work host keeps every workspace something has run in, and every one the
user adds, and lists each whether anything runs there or not, until the user
removes it. Removing a workspace ends what still runs there. Something
starting in a removed workspace keeps it again.

## Alternatives considered

- Keep a workspace for a while after its last session ends. The tree still
  changes under the user, only later, and how long to wait has no right answer.
- Keep the workspaces in the window. Each window and each computer reaching
  the work machine would list different workspaces of the same work.
- Keep only the workspaces the user adds. A repository worked in from the
  picker would still vanish once its sessions end, which is what the user
  objected to.

## Consequences

- The tree grows with every directory anything ran in until the user prunes
  it, those sessions opened by sessions included.
- A workspace whose directory is gone is no longer listed.
- Removing a workspace where nothing runs loses nothing, so it asks nothing.
