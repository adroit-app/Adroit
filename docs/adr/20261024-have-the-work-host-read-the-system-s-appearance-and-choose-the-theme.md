# Have the work host read the system's appearance and choose the theme

Status: Accepted

## Context

The user wants the console and the window light while macOS is light and
dark while it is dark, switching when macOS switches. The theme is not only
the window's: the work host colours every session and shell it starts, and
tells a program the theme's colours from the first time it asks, with no
window open as often as with one.

## Decision

The settings name a light theme and a dark theme, and whether to follow the
system or keep to one of them. The work host reads the system's appearance
itself, again as it reads the configuration again, chooses the theme in force
from the settings and that appearance, and sends the windows the theme it
chose with the appearance it read. Every session, shell and window is
coloured by that one choice.

## Alternatives considered

- Each window reports the appearance it sees, and the work host chooses by
  it. With no window open the host could not tell, and sessions started then
  would take the wrong theme; windows on different machines could disagree.
- One theme setting holding both, as `light:X,dark:Y`. It saves no work, since
  something still has to read the appearance, and it gives the Settings
  window a string to take apart instead of two choices.

## Consequences

- A session started with no window open takes the theme of the appearance the
  system is in.
- A change of the system's appearance reaches open consoles and windows with
  no window having to notice it.
- Each theme is known as light or dark, so the Settings window can offer for
  each choice only the themes of its kind.