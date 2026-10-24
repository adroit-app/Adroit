# Terminal sessions

Shell, Neovim and command terminals are terminal sessions, separate from
agent conversations. They appear beside agents in the workspace where they
run. A shell uses the account's login shell unless configured otherwise;
Neovim uses the user's normal configuration. A command terminal runs any
program through the shell, as an interactive login shell would run it, with
the environment a shell terminal gets; it is listed among the shells under the
name of its program unless given another. Embedded Neovim for prompts and
console reading is a window component, not a terminal session.

Closing a terminal's windows hides it and leaves its program running. Its tree
row opens it again. Stop Terminal (`terminal.stop`) explicitly ends the
selected or shown terminal. Quitting the program also ends it. Closing the
window leaves terminals running.

Terminal resources have identities independent of their names and working
directories. A terminal is not an agent session: it neither starts sessions
nor sends or receives messages between them.

## Commands

These commands are run by the instance's work host, started when none runs,
and work without a window:

```sh
adroit terminal start --program shell --dir /src/api --json
adroit terminal start --program neovim --dir /src/api --json
adroit terminal start --program command --dir /src/api --json -- claude --debug
adroit terminal list --json
adroit terminal input --terminal <resource-id> --text "hello" --keys Tab Enter --json
adroit terminal screen --terminal <resource-id> --json
adroit terminal stop --terminal <resource-id> --json
```

`start` accepts an optional `--name`. Without `--json` it prints the new
resource ID. `list` prints resource IDs and names separated by a tab, one per
line; `screen` prints the visible screen without trailing empty rows. A
resource is addressed by ID, not by a name or the caller's directory.
`start` creates a terminal in the given directory, or the caller's current
directory when omitted. A command terminal takes its program and arguments
after `--`, and only it does.

`input` sends what it is given to the terminal's program, in the order given,
and returns once the terminal has taken it. `--text` is typed as it is, so a
newline in it is Enter; `--paste` is pasted, bracketed when the program asks
for it; `--keys` presses each key it names, written as the keys file writes a
key ([keys](keys.md#keys)): `Tab`, `Shift+Tab`, `Ctrl+g`, `Escape`,
`Enter`. A key reaches the program as it would from the window, in the
keyboard protocol the program asked for. A key that never reaches a program,
such as one held with Command, is refused, and nothing is sent. `screen`
reads the screen as it is and does not wait.

A JSON resource contains `resource_id` (UUID), `name`, `cwd`, `program`
(`shell`, `neovim` or `command`), and for a command terminal `command`, its
program and arguments. `list --json` returns an array of these resources.
`screen --json` returns `resource_id` and `lines`; `input --json` returns
`resource_id` and `sent`; `stop --json` returns `resource_id` and `stopped`.
Without `--json`, `input` and `stop` print the resource ID.

Invalid arguments exit with status 2. A missing resource, failed launch or
unreachable daemon exits with status 1. The old `adroit shell list` and
`adroit shell screen` commands are removed.

## The window sockets

Each window of an instance answers the editor handoff and foreground
requests on a socket of its own, in `$ADROIT_WINDOWS`, or the `windows`
folder of its instance's state folder. These requests do not control
terminal resources.

Claude Code's external editor uses `adroit editor <file>` to hand over its
input. Every agent session Adroit starts — from the window, from
`adroit session start`, or by another session — names it as
`$VISUAL` and `$EDITOR` once the window has written it, keeping the user's own
editor in `$ADROIT_USER_EDITOR`; every terminal keeps the user's
editor. A window asks for the input only of a session started this way, and
`adroit editor` offers the file to each window of the instance in turn, so
the input reaches the window that asked for it. A JSONL request
`{"op":"editor","path":"/tmp/prompt.md"}` receives `{"taken":2}` when two
lines were taken, or `"declined"` when the window did not request the input.
A file every window declines opens in `$ADROIT_USER_EDITOR`.
`{"op":"show"}` brings the window to the front and receives `"shown"`.
