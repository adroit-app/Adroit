# Show every environment in one window's tree

Status: Accepted

## Context

Each environment opened in a window of its own. The sessions waiting on the
user were spread over those windows, so finding the next one to answer meant
going through each of them, and on macOS every window process stood in the
Dock as an application of its own. The tree already shows work grouped by
where it is.

## Decision

A window shows the environments open in it as the top level of its tree, each
over a connection of its own that keeps its environment. Work never moves
between environments: each session, draft and send belongs to the environment
it was made in. What waits on the user is gathered across the environments.

## Alternatives considered

- Keep a window per environment, and hold every window in one process. Only
  macOS would show one application, everything the framework keeps once per
  process would have to be shared out by window, one window's failure would
  take every environment down, and what waits on the user would still be
  split by window.
- Keep a window per environment. The user goes through the windows to find
  what waits on them.
- Merge the environments' work into one tree without headings. The same
  directory on two machines, or in two environments of one machine, could not
  be told apart.

## Consequences

- A connection lost or reconnecting leaves the window's other environments
  usable.
- Work is known by its environment together with its own identity.
- What the window can do with an item, such as opening its files with local
  tools, follows the item's environment, not the window's.
- New work chooses its environment where it chooses its directory, and work
  settings are changed for an environment chosen in the window.
- Each window is still a process of its own, as
  20261024-run-each-window-of-an-instance-as-a-process-of-its-own.md decides.
