# Sessions

Every session adroit starts, and every terminal, runs in a host of adroit's
own, not in the window ([the hosts](#the-hosts)). Closing the window leaves
them running, their scrollback kept; opening it again lists them as they are
and shows each one from where it stands, starting with what it showed when it
closed, if that still runs.

## Agents

A session runs Claude Code or Codex CLI in its own terminal interface. Each
session is given its profile, and with it its agent and connection
([settings](settings.md#profiles-and-connections)), while it is still a
draft, before its first prompt is sent. The empty window and drafts offer
the agents and every profile not archived, each named with the connection
it runs in when that is not its agent's own environment. `session.agent_next` and `session.agent_prev` switch the agent,
which starts from its default profile, and `session.profile_next` and
`session.profile_prev` switch the profile. What is chosen, by the form or by
key, is what new sessions start with while the window is open. Reopening the
app starts with the default profile, which Settings sets under General.

The draft also offers a model and permissions for that launch. Changing the
profile or the agent keeps its prompt and attachments, and keeps the model
and permissions while the agent stays the same; to another agent they go
back to the defaults of the profile chosen, and the status line says so. The
empty window offers them too, and the next new session starts with what was
chosen there. The usage, the `/` commands and the history a draft offers are
those of the connection its profile runs in. A session keeps its agent and
profile once it runs, whatever the default becomes. Both kinds can run in
the same workspace.

A session also keeps the profile it started from, and the connection that
runs in ([settings](settings.md#profiles-and-connections)), whatever the
settings say since, and after the work host starts again: messages are
handed to it, its conversation, history and processes are read, and its
starting dialogs are answered, as the settings it was started with say. The
tree and the strip of a window showing it name the session's agent and,
when it is called otherwise, its profile; the details name its profile and
its connection.

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

The sessions and terminals of an instance, and the commands run in them,
belong to it: they see only its sessions and terminals, and the sessions they
start are its own. Settings and keys are the same in every instance.

## Windows

Opening adroit while a window of the instance is open brings that window to
the front instead, the one opened first when there are several. New Window
(`window.open`), or `adroit --new-window` with or without `--instance`,
opens another window of the instance. The windows of other instances open
beside them.

Every window of an instance shows the same work: a session or a terminal
started, ended or renamed in one is seen in the others. How a window shows
it is its own: the tree's folds, selection and filter, the content's tabs and
windows, the panels, the details, the picker, and the prompt. Closing a window
leaves the others as they are, and the sessions run on with no window open.

Each window holds a number, the lowest none of the instance's open windows
holds. A window keeps, under its number, the layout of its side panels and
its content when it closed ([content](content.md#across-runs)), and the next window opened
with that number shows them again. Windows are not opened again together:
opening adroit opens one, and each New Window the next.

### The window sockets

Each window answers the editor handoff and foreground requests on a socket
of its own, in `$ADROIT_WINDOWS`, or the `windows` folder of its instance's
state folder. These requests do not control sessions or terminals.

Claude Code's external editor uses `adroit editor <file>` to hand over its
input. Every agent session adroit starts, from the window, from
`adroit session start` or by another session, names it as `$VISUAL` and
`$EDITOR`, keeping the user's own editor in `$ADROIT_USER_EDITOR`; every
terminal keeps the user's editor. A window asks for the input only of a
session started this way, and `adroit editor` offers the file to each window
of the instance in turn, so the input reaches the window that asked for it. A
JSONL request `{"op":"editor","path":"/tmp/prompt.md"}` receives
`{"taken":2}` when two lines were taken, or `"declined"` when the window did
not ask for the input. A file every window declines opens in
`$ADROIT_USER_EDITOR`. `{"op":"show"}` brings the window to the front and
receives `"shown"`.

## The hosts

Two processes of adroit's own run each instance's work, apart from the
windows, and keep running when they close.

- The terminal host, `adroit daemon`, holds every session's and terminal's
  pseudo-terminal, and nothing else. It listens on `$ADROIT_DAEMON`, or
  `$XDG_STATE_HOME/adroit/felis/daemon.sock` for the default instance and
  `$XDG_STATE_HOME/adroit/instances/<name>/felis/daemon.sock` for another
  (`$XDG_STATE_HOME` being `~/.local/state` when unset). Windows send it keys
  and draw its screens directly ([terminal protocol](terminal-protocol.md)).
- The work host, `adroit host`, holds the work: it starts and lists every
  session and terminal, reads the files, repositories and settings the windows
  show, keeps what is written in the prompts, lets sessions start and message
  sessions ([sessions that start sessions](delegation.md)) and judges how each
  session it started stands. It listens on `$ADROIT_HOST`, or
  `host/host.sock` in the same state folder as the terminal host's `felis`
  folder. Windows and the `adroit` commands reach it there
  ([work protocol](work-protocol.md)).

The folder of either socket has to be yours and open to no one else: adroit
makes it so when it makes the folder, and refuses one already there that is
not, leaving it as it is.

A window works with a host whenever they share a version of its protocol,
whichever build of adroit either is, so an update of adroit that leaves the
protocols alone asks nothing of the user. The window starts the work host of
its instance when none answers, and the work host starts the terminal host.
A work host the window cannot work with, or one of another build, the window
replaces without asking; nothing ends with it. A terminal host the window
cannot work with the window never replaces on its own: it shows a banner
offering to restart it, with the build the host says it is.

`adroit daemon stop` stops the terminal host of the instance and waits until
its process is gone; `adroit daemon restart` stops it, if one runs, and starts
another. Either ends every session and terminal the terminal host holds, with
what they scrolled back. A terminal host too old to be asked to stop is
stopped by signal. `adroit host stop` and `adroit host restart` do the same
for the work host, and end no session. Stopping where no host runs is not an
error.

The window restarts the terminal host with Restart Session Daemon
(`daemon.restart`), only after the user confirms that every session ends.

A session ends when its program exits or when it is ended from adroit. Its
console then says so, and it is listed no more.

## Resuming a past conversation

Claude Code and Codex keep each conversation after its session ends, and
adroit lists only the sessions it runs. Resume Conversation in the Session
menu, or `session.resume`, opens a list of the conversations both agents kept,
in every connection a profile runs in,
for the directory the focused window works in, the newest first, each with its
agent and profile, the connection that keeps it when that is not the agent's
own environment, its title or first prompt, its directory and when it was
last talked in. `Ctrl+a` lists those of every directory instead, and pressed again only
the directory's; with nothing open, every directory's are listed. Typing
narrows the list by a fuzzy match on the title and the directory. `Ctrl+n`
and `Ctrl+p` (or the arrow keys, `Tab` and `Shift+Tab`) move, `Enter` chooses,
and `Escape` closes the list.

Choosing a conversation resumes it in a new session of its agent, in the
directory it ran in, with the agent's own resume, in the connection that
keeps it and from a profile that runs there; when the settings no longer
hold that profile in that connection, the resume is refused, naming why,
and never runs under another login. Nothing running is stopped
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
