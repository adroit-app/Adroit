# Keep the images a console drew in memory while adroit runs

Status: Accepted

## Context

A program draws images in its console through the kitty graphics protocol, and
the console holds one only while the program keeps it. Agents redraw often, so
a diagram can be gone a few turns later, and no transcript records it.

## Decision

adroit keeps a copy of each image a session's console receives, in memory, for
as long as it runs, within bounds per session and in total past which the
oldest go.

## Alternatives considered

- Show only the images the console still holds. Those are the ones the user can
  already scroll to.
- Write the copies to disk. They may show anything a session worked on, and
  files left behind need their own rules for where they live and when they go.
- Have felis keep deleted images. felis follows kitty, which frees them; keeping
  them is the lister's choice, not the terminal's.

## Consequences

- Images drawn while adroit is detached are kept only if the console still
  holds them when it attaches again.
- Quitting adroit forgets every kept image.
