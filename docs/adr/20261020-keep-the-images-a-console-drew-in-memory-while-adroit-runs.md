# Keep the images a console drew in memory while adroit runs

Status: Accepted

## Context

A program in a session draws images in its console through the kitty graphics
protocol, and the console holds an image only while the program keeps it: it
goes once the program deletes it, or once its lines leave the scrollback.
Claude Code redraws its screen often, so a diagram it drew can be gone a few
turns later, and nothing else records it: the transcript has no trace of an
image drawn this way.

## Decision

adroit keeps a copy of each image a session's console receives, as it
arrives, in its own memory, for as long as adroit runs. The copies are
bounded: the newest few of each session, and a total size across sessions past
which the oldest go.

## Alternatives considered

- Show only the images the console still holds. Those are the ones the user can
  already see by scrolling; the ones worth listing are the ones gone.
- Write the copies to disk, to outlive adroit. The images may show anything a
  session worked on, and files left behind would need their own rules for where
  they live and when they go; a running adroit is enough to look back over a
  session's work.
- Have felis keep images past their deletion. felis follows kitty, which frees
  an image once it is deleted; keeping them is a choice of the window that
  lists them, not of the terminal.

## Consequences

- Images drawn while adroit is not attached to a session are kept only if the
  console still holds them when adroit attaches again.
- Quitting adroit forgets every kept image.
- Memory spent on kept images is bounded whatever programs draw.
