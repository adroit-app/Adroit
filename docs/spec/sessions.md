# Sessions

Every session adroit starts, and every shell, runs in a felis daemon of
adroit's own, not in the window. Closing the window leaves them running, their
scrollback kept; opening it again lists them as they are and shows each one
from where it stands, starting with the session or shell it showed when it
closed, if that still runs.

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

## Started by a session

A session can start another, and so can any program running in one:

```console
$ adroit session start --dir ~/src/api -- "Find why the login test is flaky"
find-login-test
```

The new session runs in the daemon like one started from the window, whether
the window is open or not, and the window lists and shows it the same way. The command
prints the name it goes by, which is also the name Claude Code's messaging
between sessions addresses it by. The prompt is read from standard input when
none follows `--`.

- `--dir <path>` is where it works, the current directory when not given.
- `--name <name>` names it; without one it is named from its prompt, as the
  window names a session. A name already taken gets a number.
- `--read-only` starts it as one that only reads, as `session.new_reader`
  does ([keys](keys.md)).

Run from a Claude Code session, the new session is known to have been started
from that one. The sessions adroit starts are told of the command, so Claude
can hand work to a session of its own instead of an agent inside itself, and
hear back from it by Claude Code's messaging.

When the daemon cannot be reached, the command fails with a message on
standard error and exit status 1.

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
as the daemon keeps it. Claude Code reports it itself. A session whose program
reports nothing shows as idle.
