# Sessions

Every session adroit starts, and every shell, runs in a felis daemon of
adroit's own, not in the window. Closing the window leaves them running, their
scrollback kept; opening it again lists them as they are and shows each one
from where it stands, starting with the session or shell it showed when it
closed, if that still runs.

## Agents

A session runs Claude Code or Codex CLI in its own terminal interface. Each
session is given its agent while it is still a draft, before its first prompt
is sent. The empty window and drafts offer the agent choices, and
`session.agent_next` switches between them. Choosing an agent, by button or key, also makes it the default
for new sessions while the window is open. Reopening the app starts with the
default in Settings under General, or `agent` in [settings.toml](settings.md).
The draft also offers a model and permission profile for that launch; changing
these choices keeps its prompt and attachments. The empty window offers the
model and permission profile too, and the next new session starts with
what was chosen there. A session keeps its agent once it runs, whatever the default becomes. Both
kinds can run in the same workspace.

Codex sessions use the embedded prompt and report their working and waiting
states through the terminal, including permission and question prompts. Their
output stays in the terminal scrollback for scrolling and reading in Neovim. A
read-only Codex session uses the read-only sandbox. Codex's own configuration
controls authentication; Adroit can override its model, reasoning effort,
approvals and sandbox for each launch. Claude's model,
permission mode and denied tools do not apply to Codex.

Codex sessions started by Adroit list their conversation's artifacts, images,
links and files in the [overview](details.md). The list follows the current
conversation, including after switching conversations in Codex.
For a Codex session already running when Adroit was updated, run
`/title app-name thread-id` in Codex to enable the overview's conversation list.

Adroit currently lists only the Codex sessions it starts. Discovery and
takeover of external Codex sessions and Codex prompt history in the embedded
prompt are not available. Codex menus, approvals and questions take keys
directly while they are open. Use Codex's own interface for its commands and
menus:
`Ctrl-g i` passes keys through until `Esc`. Renaming in Adroit changes the
Codex session's sidebar name only. A message with a leading `!` is pasted into
Codex as it was written, so what Codex does with it is up to Codex.

## Instances

adroit runs as named instances, each with its own daemon, sessions and
window, apart from the others. `adroit --instance <name>` opens the window of
the instance `name`, as does `adroit` with `$ADROIT_INSTANCE` set; without
either it is the instance `default`. A name is made of letters, digits, `-`
and `_`, up to 32 of them.

An instance has one window at most. Opening its window while it is open
brings the open one to the front instead. The windows of other instances open
beside it.

The sessions and shells of an instance, and the commands run in them, belong
to it: they see only its sessions and shells, and the sessions they start are
its own. Settings and keys are the same in every instance.

## The daemon

The window starts the daemon of its instance when none answers, as
`adroit daemon`, and leaves it running when it closes. It listens on
`$ADROIT_DAEMON`, or `$XDG_STATE_HOME/adroit/felis/daemon.sock` for the
default instance and `$XDG_STATE_HOME/adroit/instances/<name>/felis/daemon.sock`
for another (`$XDG_STATE_HOME` being `~/.local/state` when unset).
The folder of the socket has to be yours and open to no one else: adroit makes
it so when it makes the folder, and refuses one already there that is not,
leaving it as it is.

`adroit daemon stop` stops the daemon of the instance and waits until its
process is gone; `adroit daemon restart` stops it, if one runs, and starts
another. Either ends every session the daemon holds, agents and shells alike,
with what they scrolled back. A daemon too old to be asked to stop is stopped
by signal. Stopping where no daemon runs is not an error.

The window restarts the daemon with Restart Session Daemon
(`daemon.restart`), only after the user confirms that every session ends.
When the window finds the daemon runs another build of adroit than its own,
or one too old to say which, it shows a banner offering that restart; it
never restarts the daemon on its own.

A session ends when its program exits or when it is ended from adroit. Its
console then says so, and it is listed no more.

## Started by a session

A session can start sessions of its own, message other sessions and report
back to the one that started it, and `adroit session start` starts one from a
terminal ([sessions that start sessions](delegation.md)).

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
as the daemon keeps it. When Codex reports no status, its visible activity and
requests for approval or input supply the state instead. A session Adroit
started shows as starting until its program reports a state or its visible
activity supplies one.

The completion check marks a result not yet viewed in this window. Viewing
the session acknowledges it; a later turn completed out of view can be marked
again. A window in the background does not acknowledge completions.

## Renaming a session

Double-clicking a session's name in the sidebar or its tab edits that name
in place. The rename action from the keyboard or menu opens the same editor.
Enter or leaving the field accepts the name; Escape cancels. An empty or
unchanged name leaves the session's name as it was.
