# Keep scratch sessions in a directory of adroit's own

Status: Accepted

## Context

Every session runs in a directory the user chose first. Talking to Claude
about something no project holds needs none, but Claude Code still runs in
one: it reads what is there and keeps the conversation by that directory, so
`/resume` finds the conversation only from the same directory. adroit lists
sessions by the directory they run in, one workspace for each directory
outside git.

## Decision

A session started without a directory chosen runs in a new directory of its
own under one root that adroit keeps in the user's data directory. The root
is one workspace in the sidebar, holding everything under it. adroit removes
none of these directories.

## Alternatives considered

- A temporary directory. The system clears it, and the conversation can no
  longer be resumed from where it ran.
- The home directory. Claude Code would read everything under it, and every
  such session would share one directory.
- One fixed directory shared by every such session. Files one session writes
  meet those of every other.
- One workspace for each directory. Each session would add a workspace to the
  sidebar, crowding out the projects.

## Consequences

- The directories add up until the user removes them.
- Claude Code asks whether to trust each new directory, unless the root or a
  directory above it is trusted.
- Whatever runs under the root is in that one workspace, whatever git says
  of it.
