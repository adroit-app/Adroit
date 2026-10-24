# Sessions that start sessions

Every session Adroit starts can hand work to sessions of its own, send
messages to any other session Adroit started, and hear back. There is no role
to choose: the user asks a session for something, and the session decides
whether to do it itself, give it to its agent's own subagents, or start
sessions for it. A session started this way is an ordinary session, read and
steered in its console like any other.

A session the user started from the window, or from `adroit session start` in
a terminal of their own, can start sessions. A session started by another
cannot: when its work outgrows it, it tells its starter what it has found and
how it would divide the rest, and the starter decides.

## What a session can do

Each session Adroit starts has these as tools of its agent, through an MCP
server named `adroit`. The tools act as the session that calls them; a session
cannot act as another.

- **`start_session`** starts a session with a first prompt. It takes the
  directory to work in (the caller's own when omitted), whether to work in a
  new worktree, the revision that worktree starts from, the profile, the
  agent, the model, the permission profile and a name. It returns the new
  session's id and name. It starts from the profile it names, which has to
  start the agent when both are named; else from the agent's default profile,
  else from the default profile ([settings](settings.md#profiles-and-connections)).
  It never starts from the caller's profile unless it names it. A profile that
  is not defined, is archived or starts another agent is refused, and a
  refused start leaves no record and runs nothing.
  A new worktree is a jj workspace when the directory belongs to jj, otherwise
  a git worktree; it starts from the given revision, or from the caller's
  current change or commit. Outside version control, a new worktree is
  refused. A worktree that cannot be made refuses the start; the session never
  falls back to the caller's directory. A model must be one its agent lists
  (see `models`); any other is refused, naming the listed ones.
- **`send`** sends a message to another session by its id. Any session Adroit
  started, running or not yet ready, can be sent to; one that has ended
  cannot.
- **`sessions`** lists the sessions Adroit runs, each with its id, name,
  workspace, the directory it runs in, how it stands and which session started
  it. It can be narrowed to one repository or directory.
- **`report`** sends the caller's report to the session that started it. Once
  the turn that reported is over, Adroit ends the reporting session. Its
  worktree, and everything in it, stays.
- **`stop_session`** ends a session the caller started.
- **`models`** lists, for each agent, the models its provider reports, and
  which agent a start without one uses. An agent whose provider has not
  answered has no list, and its models are not checked.

At most `max_started` started sessions run at once
([settings](settings.md#settingstoml)); a start beyond it is refused with the
limit named, and sessions the user started never count against it.

## Messages

A message is handed to the recipient's agent through its own way of hearing
from outside: Claude Code's channel, or Codex's queue of incoming
notifications. It reaches the agent between turns and starts one, headed by
who sent it: the sender's id, name and directory. A message sent while the
recipient waits on a dialog or is still starting is handed over once it is
ready. Handing it over is receipt; no acknowledgement is asked for, and
receipt says nothing about whether the work it asks for was done.

adroit keeps every message and who started whom, so both outlive the window,
the sessions and the work host that relays them. A report sent to a session that has since ended
is kept, and shown in the details of both ([details](details.md)). A message
carries text; larger material is referred to by its path.

Sessions Adroit did not start, such as Claude Code sessions in other
terminals, can neither send nor receive.

## What sessions are told

Adroit gives every session it starts the same instructions about these tools.
A session does work itself by default and starts sessions for work that can
run apart from the rest, that runs long, that needs a worktree of its own or
another agent, or that the user may want to watch and step into. A started
session finishes and checks its work before it reports. Before touching
something it shares with unrelated sessions in the same repository, such as
the checkout itself, a branch, or a port, a session tells those sessions what
it is about to do, and afterwards what it did; it neither waits for their
answer nor takes a lock. Sessions send only what the recipient has to act on:
no progress notes and no replies that only acknowledge.

Where the instructions name an `adroit` command, such as `adroit terminal`
for probes, they name the adroit whose work host started the session by its
absolute path, so a session runs it from any directory, whatever its `PATH`:
the installed app's own binary, or a development build's own when it runs an
instance of its own.

## Starting up

Adroit watches each session it starts until its agent is ready. When the
agent waits on a dialog — trust, permissions, a model change, or any dialog
Adroit cannot name — the session shows it as waiting on you, and the session
that started it is told; so it is when the session neither becomes ready nor
shows a dialog for long enough to count as stalled, and when a session it was
told of either way becomes ready after all. A session that becomes ready
without either is not announced: its starter next hears from it in its report
or a message.
Adroit never answers trust or permission questions for the user.

Claude Code warns at every launch that Adroit's channel is a development
channel. Adroit confirms that warning, once a launch and only when it names
Adroit's own channel and nothing else, unless `confirm_channel_warning` is
turned off ([settings](settings.md#settingstoml)); with it off, the warning
waits for the user like any other dialog.

## Ending

A session that reports ends after its turn. Ending a session, by the user or
by its starter, does not end the sessions it started; they run on until they
report or are ended, and their reports stay readable. Closing the window ends
nothing.

## From a terminal

The same start is available as a command. The instance's work host runs it,
started when none runs, so it works with the window closed:

```console
$ adroit session start --dir ~/src/api --worktree -- "Find why the login test is flaky"
find-login-test
```

It prints the new session's name. The prompt is read from standard input when
none follows `--`.

- `--dir <path>` is where it works, the current directory when not given.
- `--worktree` works in a new worktree of that directory, and `--base <rev>`
  names the revision it starts from.
- `--name <name>` names it; without one it is named from its prompt, as the
  window names a session. A name already taken gets a number.
- `--profile <id>` starts it from that profile, and `--agent claude|codex`
  from that agent's default profile; with both, the profile has to start the
  agent. Without either it starts from the default profile.
- `--model <model>` selects the model for this launch without changing the
  defaults for other sessions. If neither specifies a model, the agent uses its
  own default. The agent's TUI can still change its model after launch.
- `--permissions default|read_only` selects the launch's permission profile.
  Read-only uses Codex's read-only sandbox. A read-only Claude Code session
  loses its file-editing tools and runs its shell commands in Claude Code's
  sandbox, which keeps them from writing to its directory or the repository's
  history; the user's and the repository's Claude Code settings cannot loosen
  this. Where that sandbox cannot run, the session fails to start.
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

Run from a shell inside a session Adroit started, the command acts as that
session: the new session is started by it, and it is refused when that session
was itself started by another. When the work host cannot be reached, the command
fails with a message on standard error and exit status 1.

The identity a session acts as comes from its environment. It keeps sessions
from being mistaken for one another; it does not protect against programs that
run as the same user and set out to impersonate a session.
