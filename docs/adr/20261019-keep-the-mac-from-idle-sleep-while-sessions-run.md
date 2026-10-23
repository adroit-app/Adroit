# Keep the Mac from idle sleep while sessions run

Status: Accepted

## Context

Sessions are meant to go on without the user at the screen. Idle sleep
suspends the daemon and its sessions as soon as the user walks away. The window
may be closed meanwhile; only the daemon lives as long as the sessions.

## Decision

The daemon holds a macOS power assertion against idle system sleep while any
of its sessions or shells has a program running, and lets it go when none
does. The display still sleeps. A setting, on by default, turns this off.

## Alternatives considered

- Leave it to the user's own tools or energy settings. Easy to forget, or the
  Mac stays awake whether anything runs or not.
- Keep the display awake, or prevent all system sleep. Sessions need neither.
- Have the window hold the assertion. It may be closed while sessions run.

## Consequences

- An idle session keeps the Mac awake too, until it ends.
- Closing the lid on battery, sleeping by hand and the system's own emergencies
  still sleep the Mac.
- Nothing is held outside macOS.
