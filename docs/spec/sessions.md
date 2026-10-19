# Sessions

Every session adroit starts, and every shell, runs in a felis daemon of
adroit's own, not in the window. Closing the window leaves them running, their
scrollback kept; opening it again lists them as they are and shows each one
from where it stands.

## The daemon

The window starts the daemon when none answers, as `adroit daemon`, and leaves
it running when it closes. It listens on `$ADROIT_DAEMON`, or
`$XDG_STATE_HOME/adroit/felis/daemon.sock`
(`~/.local/state/adroit/felis/daemon.sock` when `$XDG_STATE_HOME` is unset).
The folder of the socket has to be yours and open to no one else: adroit makes
it so when it makes the folder, and refuses one already there that is not,
leaving it as it is.

A session ends when its program exits or when it is ended from adroit. Its
console then says so, and it is listed no more.

## Keeping the Mac awake

While a session or a shell runs, the window open or not, the Mac does not go
to idle sleep, so they go on with no one at it; the display still sleeps as
the Energy settings say. Closing the lid on battery, sleeping it by hand and
the system's own emergencies still sleep it. `pmset -g assertions` lists what
keeps it awake under adroit's name. Within seconds of the last one ending, or
of `keep_awake` being turned off in the [settings](settings.md), the Mac
sleeps when idle again.

## How a session stands

The state a session shows, working, waiting on you and for what, done or
failed, is what its program reports with the
[OSC 7501 program status protocol](https://www.superlogical.com/rex/docs/build/program-status),
as the daemon keeps it. Claude Code does not report it itself, so the sessions
adroit starts run hooks that report it for Claude Code. A session whose
program reports nothing shows as idle.
