# Keep the images a console drew in the window's memory while it is open

Status: Accepted

## Context

A program draws images in its console through the kitty graphics protocol, and
the console holds one only while the program keeps it. Agents redraw often, so
a diagram can be gone a few turns later, and no transcript records it. The
window is what receives the images, as it draws the console.

## Decision

The window keeps a copy of each image a session's console sends it, in
memory, for as long as it is open, within bounds per session and in total
past which the oldest go.

## Alternatives considered

- Show only the images the console still holds. Those are the ones the user can
  already scroll to.
- Write the copies to disk. They may show anything a session worked on, and
  files left behind need their own rules for where they live and when they go.
- Have felis keep deleted images. felis follows kitty, which frees them; keeping
  them is the lister's choice, not the terminal's.

## Consequences

- Images drawn while no window shows the session are kept only if the console
  still holds them when a window attaches.
- Closing a window forgets the images it kept, and each window keeps only those
  it was sent.
