# Run each window of an instance as a process of its own

Status: Accepted

## Context

Any number of windows may show one instance, as
20261023-keep-each-instance-of-adroit-apart-with-hosts-of-its-own.md decides.
The window's application framework keeps some things once per process: the
menu bar, the Settings window, the socket a window answers the editor and
other programs on.

## Decision

Each window is a process of its own. Opening another window of an instance
starts another window process for it, which takes the lowest number no open
window of the instance holds and keeps its own state under that number.

## Alternatives considered

- Many windows in one process. Everything the framework keeps once per
  process would have to be shared out by window, and one window's failure
  would take every window down.
- A window process per instance, as before. The same work could not be shown
  twice.

## Consequences

- Each window has a menu bar, a Settings window and a socket of its own, and
  macOS shows each window process as an application of its own.
- A window closing or failing leaves the other windows as they are.
- What concerns the whole instance, such as telling that a session waits on
  the user, falls to one window, the one with the lowest number.
