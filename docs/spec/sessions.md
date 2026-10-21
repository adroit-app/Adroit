# Sessions

Every session adroit starts, and every shell, runs in a felis daemon of
adroit's own, not in the window. Closing the window leaves them running, their
scrollback kept; opening it again lists them as they are and shows each one
from where it stands, starting with the session or shell it showed when it
closed, if that still runs.

## Agents

Sessions started from Workspaces default to Standalone: they run without
controller ownership, an orchestration bridge, or Leader / Worker role policy.
Sessions started from Leader / Workers default to Leader. The draft offers this
choice before launch and keeps it when switching views or agents.

A session runs Claude Code or Codex CLI in its own terminal interface. Each
session is given its agent while it is still a draft, before its first prompt
is sent. The empty window and drafts offer the agent choices, and
`session.agent_next` switches between them. Choosing an agent, by button or key, also makes it the default
for new sessions while the window is open. Reopening the app starts with the
default in Settings under General, or `agent` in [settings.toml](settings.md).
The draft also offers a model and permission profile for that launch; changing
these choices keeps its prompt and attachments. The empty window offers the
mode, model and permission profile too, and the next new session starts with
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

A session ends when its program exits or when it is ended from adroit. Its
console then says so, and it is listed no more.

## Started by a session

A session can start another, and so can any program running in one.
Use --agent claude or --agent codex to override the default agent:

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
- `--model <model>` selects the model for this launch, overriding the selected
  agent's configured model without changing defaults for other sessions. If
  neither specifies a model, the agent uses its own default. The agent's TUI
  can still change its model after launch.
- `--permissions default|read_only` selects the launch's permission profile.
  Read-only currently uses Codex's read-only sandbox. Claude Code read-only
  requests fail explicitly: plan mode alone is not read-only enforcement.
- `--permissions-file <path>` reads custom permissions from JSON instead of a
  named profile. These are provider-specific options, not a read-only promise.
  The file is one of:

  ```json
  {"agent":"claude","permission_mode":"plan","disallowed_tools":["Edit","Write"]}
  ```

  ```json
  {"agent":"codex","approval_policy":"on-request","sandbox_mode":"workspace-write"}
  ```

  The provider must match `--agent`; unknown fields are rejected.

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
