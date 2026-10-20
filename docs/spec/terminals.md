# Terminal sessions

Shell and Neovim are terminal sessions, separate from agent conversations.
They appear beside agents in the workspace where they run. A shell uses the
account's login shell unless configured otherwise; Neovim uses the user's
normal configuration. Embedded Neovim for prompts and console reading is a
window component, not a terminal session.

Closing a terminal tab hides it and leaves its program running. Its sidebar
row opens it again. Stop Terminal (`terminal.stop`) explicitly ends the
selected or shown terminal. Quitting the program also ends it. Closing the
window leaves terminals running.

Terminal resources have identities independent of their names and working
directories. They may carry an explicit request and worker association; a
shared terminal without one remains unassigned. A terminal does not count as
an agent or worker, and changing focus does not change its association.

## Commands

These commands talk to the instance's daemon and work without a window:

```sh
adroit terminal start --program shell --dir /src/api --json
adroit terminal start --program neovim --dir /src/api --json
adroit terminal list --json
adroit terminal list --run <run-id> --json
adroit terminal screen --terminal <resource-id> --json
adroit terminal stop --terminal <resource-id> --json
```

`start` accepts an optional `--name`. Without `--json` it prints the new
resource ID. `list` prints resource IDs and names separated by a tab, one per
line; `screen` prints the visible screen without trailing empty rows. A
resource is addressed by ID, not by a name or the caller's directory.
`start` creates an unassigned terminal in the given directory, or the caller's
current directory when omitted.

A JSON resource contains `resource_id` (UUID), `name`, `cwd`, `program`
(`shell` or `neovim`), and `binding` (null or an object with `run_id` and an
optional `worker_id`). `list --json` returns an array of these resources.
`screen --json` returns `resource_id` and `lines`; `stop --json` returns
`resource_id` and `stopped`.

Invalid arguments exit with status 2. A missing resource, failed launch or
unreachable daemon exits with status 1. The old `adroit shell list` and
`adroit shell screen` commands are removed.

## The window socket

The window still owns the editor handoff and foreground requests on
`$ADROIT_SOCKET`, or `adroit.sock` in its instance's state folder. These
requests do not control terminal resources.

Claude Code's external editor uses `adroit editor <file>` to hand over its
input. A JSONL request `{"op":"editor","path":"/tmp/prompt.md"}` receives
`{"taken":2}` when two lines were taken, or `"declined"` when the window did
not request the input. A declined file opens in `$ADROIT_USER_EDITOR`.
`{"op":"show"}` brings the instance's window to the front and receives
`"shown"`.
