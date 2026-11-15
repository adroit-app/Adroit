# Compose the details panel from components the user arranges into tabs on the client

Status: Accepted

## Context

The details panel showed a fixed set of sheets, each with contents chosen in
the code, and a fixed foot of usage meters under every sheet. What one person
wants in sight while a session works differs from what another wants: some
watch the machine and the account's limits, others the timeline or the
changes. A new kind of information had to be fitted into one of the fixed
sheets, and a user who wanted it beside another could not have it.

The work host reads only what the panel shows, since reading costs: a
repository read runs jj, a usage read asks the agent. Which reads to make
followed from which fixed sheet was shown.

## Decision

The details panel is made of components. A component is one piece of
information with what it needs read and what it can describe — a session, an
environment, or a GitHub pull request or issue. The user arranges components
into tabs of their own naming, and into a foot shown under every tab, once
for each of the things the panel can describe. The arrangement is a display
preference of the client computer, kept beside its other display preferences,
for every environment.

Adroit ships an arrangement that the user's replaces whole. The work host is
asked to read only what the components in sight need: the shown tab's and the
foot's.

## Alternatives considered

- Keep fixed sheets and let the user hide or reorder them. A piece of
  information still lives in one sheet only, and a new kind still needs a
  sheet chosen for it.
- Keep the arrangement in each environment's work settings. The same window
  would change under the user as they move between environments, as
  20261101-keep-display-keys-and-appearance-on-the-client-and-write-the-appearance-into-each-work-host.md
  found for keys and appearance.
- Let the user list only additions to Adroit's arrangement. What to take out
  of it and in which order cannot be said by additions, and an addition's
  place breaks as the arrangement Adroit ships changes.

## Consequences

- A component reads and draws the same wherever it is placed, so each is
  written once and tested on its own.
- Which reads the work host makes follows from the components in sight, not
  from the sheet's name; a component in no tab costs nothing.
- Keys cannot name sheets that the user may rename or remove; they name tabs
  by position.
- An arrangement naming a component this Adroit does not know is refused as
  any malformed setting is, rather than read in part.
