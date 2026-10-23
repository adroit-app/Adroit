# Keep the Mac from idle sleep while sessions run

Status: Accepted

## Context

Sessions are meant to go on without the user at the screen. Idle sleep
suspends the hosts and their sessions as soon as the user walks away. The
window may be closed meanwhile, or on another machine.

## Decision

The work host holds a macOS power assertion against idle system sleep on its
machine while any of its sessions or shells has a program running, and lets it
go when none does. The display still sleeps. A setting, on by default, turns
this off.

## Alternatives considered

- Leave it to the user's own tools or energy settings. Easy to forget, or the
  Mac stays awake whether anything runs or not.
- Keep the display awake, or prevent all system sleep. Sessions need neither.
- Have the window hold the assertion. It may be closed while sessions run, or
  keep the wrong machine awake.
- Have the terminal host hold it. A change to it would then end the sessions.

## Consequences

- An idle session keeps the Mac awake too, until it ends.
- While the work host is being replaced, nothing holds the Mac awake.
- Closing the lid on battery, sleeping by hand and the system's own emergencies
  still sleep the Mac.
- Nothing is held outside macOS.
