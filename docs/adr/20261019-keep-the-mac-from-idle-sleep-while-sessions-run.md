# Keep the Mac from idle sleep while sessions run

Status: Accepted

## Context

Sessions are meant to go on without the user at the screen: a long run, or
Remote Control waiting for input from another device. When the Mac goes to
idle sleep, every process is suspended, the daemon and its sessions with it,
so they stop as soon as the user walks away and the display goes off. The
window may be closed meanwhile; only the daemon lives as long as the sessions.

## Decision

The daemon holds a macOS power assertion against idle system sleep while any
session or shell in it has a program running, and lets it go when none does.
The display still sleeps. A setting, on by default, turns this off.

## Alternatives considered

- Leave it to the user: `caffeinate`, or never sleeping in the Energy
  settings. Easy to forget, and the second keeps the Mac awake whether
  anything runs or not.
- Also keep the display from sleeping. Sessions do not need the screen lit.
- Prevent system sleep altogether. It would hold the Mac awake against more
  than idleness, which a session does not need.
- Have the window hold the assertion. The window may be closed while the
  sessions run.
- Run `caffeinate -i -w <daemon pid>` beside the daemon. A second process to
  start, watch and stop, where IOKit's call is two functions.

## Consequences

- The Mac stays awake as long as a session or shell runs, an idle one too,
  until it ends or the setting is turned off.
- Closing the lid on battery, sleeping by hand and the system's own
  emergencies still sleep the Mac.
- Elsewhere than macOS nothing is held.
