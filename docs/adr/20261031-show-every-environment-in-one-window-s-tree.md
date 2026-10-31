# Show every environment in one window's tree

Status: Accepted

## Context

Each environment opened in a window of its own, so the sessions waiting on the
user were spread over windows, and on macOS every window process stood in the
Dock as an application of its own. Gathering the environments into one
window's tree only as the user opened them left the others out of sight, kept
one environment as the window's own that could not be closed, and made opening
another read as switching away from it. Whether an environment could be
reached was told only once the user went to it.

## Decision

A window's tree lists, at its top level, every environment the user keeps,
whether or not it can be reached, and each says how its connection stands. The
window holds a connection to each, which keeps its environment, and lists them
alike. Work never moves between environments: each
session, draft and send belongs to the environment it was made in. What waits
on the user is gathered across the environments.

## Alternatives considered

- List only the environments opened in the window, beside one that is the
  window's own. The others stay out of sight until opened, the window's own
  is treated unlike the rest, and opening one reads as switching to it.
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

- The window connects to every environment it lists. One that cannot be
  reached stays listed with the work last received from it, and the window's
  other environments stay usable.
- Adding an environment adds it to the tree, and only removing it takes it
  out.
- Work is known by its environment together with its own identity.
- What the window can do with an item, such as opening its files with local
  tools, follows the item's environment.
- New work chooses its environment where it chooses its directory, and work
  settings are changed for an environment chosen in the window.
- Each window is still a process of its own, as
  20261024-run-each-window-of-an-instance-as-a-process-of-its-own.md decides.
