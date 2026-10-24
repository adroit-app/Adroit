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
`session.agent_next` and `session.agent_prev` switch between them. Choosing an agent, by button or key, also makes it the default
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

Codex prompt history in the embedded prompt is not available. Codex menus, approvals and questions take keys
directly while they are open. Use Codex's own interface for its commands and
menus:
`Ctrl-g i` passes keys through until `Esc`. Renaming in Adroit changes the
Codex session's name in the tree only. A message with a leading `!` is pasted into
Codex as it was written, so what Codex does with it is up to Codex.

## Instances

adroit runs as named instances, each with its own hosts and sessions, apart
from the others. `adroit --instance <name>` opens a window of the instance
`name`, as does `adroit` with `$ADROIT_INSTANCE` set; without either it is
the instance `default`. A name is made of letters, digits, `-` and `_`, up to
32 of them.

Opening adroit while a window of the instance is open brings that window to
the front instead, the one opened first when there are several. New Window
(`window.open`), or `adroit --new-window` with or without `--instance`,
opens another window of the instance. The windows of other instances open
beside them.

Every window of an instance shows the same work: a session or a shell
started, ended or renamed in one is seen in the others. How a window shows
it is its own: the tree's folds, selection and filter, the content's tabs and
windows, the panels, the details, the picker, and the prompt. Closing a window
leaves the others as they are, and the sessions run on with no window open.

Each window holds a number, the lowest none of the instance's open windows
holds. A window keeps, under its number, the layout of its side panels and
what its focused window showed when it closed, and the next window opened
with that number shows them again. Windows are not opened again together:
opening adroit opens one, and each New Window the next.

The sessions and shells of an instance, and the commands run in them, belong
to it: they see only its sessions and shells, and the sessions they start are
its own. Settings and keys are the same in every instance.

## The hosts

Two processes of adroit's own run each instance's work, apart from the window,
and keep running when it closes.

- The terminal host, `adroit daemon`, holds every session's and shell's
  terminal, and nothing else. It listens on `$ADROIT_DAEMON`, or
  `$XDG_STATE_HOME/adroit/felis/daemon.sock` for the default instance and
  `$XDG_STATE_HOME/adroit/instances/<name>/felis/daemon.sock` for another
  (`$XDG_STATE_HOME` being `~/.local/state` when unset).
- The work host, `adroit host`, holds the work: it starts and lists every
  session and shell, reads what the window shows of them, lets sessions start
  and message sessions ([sessions that start sessions](delegation.md)) and
  judges how each session it started stands. It listens on `$ADROIT_HOST`, or
  `host/host.sock` in the same state folder as the terminal host's `felis`
  folder.

The folder of either socket has to be yours and open to no one else: adroit
makes it so when it makes the folder, and refuses one already there that is
not, leaving it as it is.

Each host says first which versions of its protocol it speaks, and the window
works with a host whenever they share one, whichever build of adroit either
is. A version changes only when what the protocol sends changes shape, so an
update of adroit that leaves the protocols alone asks nothing of the user.

The window starts the work host of its instance when none answers, and the
work host starts the terminal host. A work host the window cannot work with,
or one of another build, the window replaces without asking; nothing ends with
it. When the work host goes, the window starts another, connects to it and
shows the work as it holds it; a session's tools that ask meanwhile wait for
the new one, and what the window asked of the one gone and was not answered
fails with a word to the user. A terminal host the window cannot work with,
including one too old to say which versions it speaks, the window never
replaces on its own: it shows a banner offering to restart it, with the build
the host says it is.

`adroit daemon stop` stops the terminal host of the instance and waits until
its process is gone; `adroit daemon restart` stops it, if one runs, and starts
another. Either ends every session the terminal host holds, agents and shells
alike, with what they scrolled back. A terminal host too old to be asked to
stop is stopped by signal. `adroit host stop` and `adroit host restart` do the
same for the work host, and end no session. Stopping where no host runs is not
an error.

The window restarts the terminal host with Restart Session Daemon
(`daemon.restart`), only after the user confirms that every session ends.

A session ends when its program exits or when it is ended from adroit. Its
console then says so, and it is listed no more.

## Resuming a past conversation

Claude Code and Codex keep each conversation after its session ends, and
adroit lists only the sessions it runs. Resume Conversation in the Session
menu, or `session.resume`, opens a list of the conversations both agents kept
for the directory the focused window works in, the newest first, each with its
agent, its title or first prompt, its directory and when it was last talked
in. `Ctrl+a` lists those of every directory instead, and pressed again only
the directory's; with nothing open, every directory's are listed. Typing
narrows the list by a fuzzy match on the title and the directory. `Ctrl+n`
and `Ctrl+p` (or the arrow keys, `Tab` and `Shift+Tab`) move, `Enter` chooses,
and `Escape` closes the list.

Choosing a conversation resumes it in a new session of its agent, in the
directory it ran in, with the agent's own resume. Nothing running is stopped
or changed. A conversation a session of this instance already holds is not
resumed a second time: choosing it shows that session.

Conversations no one wrote a prompt in, and those Codex ran without its own
interface, are not listed.

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

Double-clicking a session's name in the tree or its tab edits that name
in place. The rename action from the keyboard or menu opens the same editor.
Enter or leaving the field accepts the name; Escape cancels. An empty or
unchanged name leaves the session's name as it was.
