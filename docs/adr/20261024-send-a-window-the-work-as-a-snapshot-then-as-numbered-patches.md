# Send a window the work as a snapshot, then as numbered patches

Status: Accepted

## Context

The work host owns the work and any number of windows show it. Each window
shows a part of it: the directories it has unfolded, the session in its
details. The work changes all the time without the window asking, as
sessions start, end and report, and a window has to show the change at once.
The work host can be replaced at any time, and a window can lose it midway.

## Decision

A window tells the work host what it shows. The host sends it the work as it
shows it, whole, and from then on every change to that as a patch, numbered in
the order the host made them. A window that finds a number missing asks for
the whole work again. What a window waits for is an ask, numbered by the
window and answered once, to it alone. A window that loses the host connects
again, takes the whole work afresh, and counts what it was waiting for as
failed.

## Alternatives considered

- Send the whole work on every change. Most changes are small, and the work
  grows with every session.
- Let the window ask for what it shows, over and over. Changes show late, and
  every window repeats the same reads.
- Let windows read a store the host writes. The window would reach the hosts'
  machine through files, which only works where it runs there too.

## Consequences

- The host keeps, for each window, what it shows and how far it has been sent.
- A window that missed changes, or met a new host, is never patched from a
  state it does not hold.
- An ask carried out by a host that went away is not known to be done, and
  the user is told so.
- Whatever the host's own state, a window is drawn from what it was sent.
