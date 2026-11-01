# Keep display, keys and appearance on the client and write the appearance into each work host

Status: Accepted

## Context

One window lists every environment the user keeps, each with work settings of
its own on its work machine. The key bindings, the appearance with its light
and dark themes, and which of the machine's meters the details show were among
those work settings, so one person at one window had them set once per
environment, and the same window took other keys and other colours depending
on the environment it was connected to. They are the person's choices at the
window, not the work machine's.

The work host still has to know the appearance and the themes: it chooses the
theme from them and the system's appearance, with no window open as often as
with one, as
20261024-have-the-work-host-read-the-system-s-appearance-and-choose-the-theme.md
decides.

## Decision

The key bindings, the appearance and its themes, and the machine meters shown
belong to the client, for every environment, and are kept on the client
computer beside its other display and input preferences. The window reads its
key bindings from the client's own file, whichever environments it shows, and
asks each work host to sample only the meters the client shows.

The appearance and its themes are edited once, on the client, and the window
writes them into each environment's work settings as it reaches one where
they differ, and again as they are changed, when the work machine can be
written to. Each work host goes on reading the
system's appearance and choosing the theme from its own settings.

## Alternatives considered

- The window decides the theme and tells each work host. It is the
  alternative 20261024-have-the-work-host-read-the-system-s-appearance-and-choose-the-theme.md
  rejected: with no window open the host could not tell, and sessions
  started then would take the wrong theme.
- Keep them as work settings, edited per environment. The person sets the
  same choice again for every environment, and the window changes under them
  as they move between environments.
- Keep the key bindings on each work machine and copy them between machines.
  The window binds keys itself, so a work machine's copy has no use but to
  disagree with the client's.

## Consequences

- An environment that cannot be reached keeps these choices editable; the
  appearance reaches it once it can be written to again.
- A work host started before the window links to it, or one the window
  cannot write to, keeps the appearance its own settings last named.
- Only the theme's name travels: a theme file of the user's own is read where
  the work host runs, so one kept only on the client is not found by another
  work machine.
- A work machine's own key bindings file is no longer read by any window.
