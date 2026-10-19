# Reading the user's shells

The shells you open in adroit are your account's login shell, started as a
login shell, whatever shell adroit itself was started from. They keep running
when the window closes, as sessions do, and the sidebar lists each under the
worktree it runs in ([workspaces](workspaces.md#the-tree)), so a shell whose
tab is out of view is a row away. Closing a shell's tab, or closing it from
its row, ends it.

The shells you open in adroit belong to a worktree. A session working in that
worktree can read them, so it can see a dev server's log or a failing command
without you copying it over.

## From a session

```console
$ adroit shell list
shell
shell-2
$ adroit shell screen shell-2
$ npm run dev
  ready on http://localhost:3000
```

- `adroit shell list` prints the names of the shells in the worktree holding
  the current directory, one per line.
- `adroit shell screen <name>` prints what that shell shows now, without the
  blank rows below the last line.

The worktree is the deepest one adroit knows that holds the current directory:
a worktree under `.claude/worktrees/` inside a main checkout sees its own
shells, not the main checkout's. Outside every worktree, or with no adroit
window open, the command fails with a message on standard error and exit
status 1.

## The socket

The window answers on a unix socket at `$ADROIT_SOCKET`, or
`$XDG_STATE_HOME/adroit/adroit.sock` (`~/.local/state/adroit/adroit.sock`
when `$XDG_STATE_HOME` is unset). Only one window answers at a time; a second
window says so and leaves the first one answering. Only the window that answers
takes text back from Claude Code's input.

A request and its answer are each one line of JSON:

```json
{"op":"list","cwd":"/src/api"}
{"shells":["shell","shell-2"]}

{"op":"screen","cwd":"/src/api","name":"shell-2"}
{"lines":["$ npm run dev","  ready on http://localhost:3000"]}

{"error":"no shell named dev in /src/api"}
```

Claude Code's external editor asks over the same socket. `adroit editor <file>`
hands over the file Claude Code wrote its input to; the window answers how many
lines it took into the prompt, and leaves the file empty when it took any:

```json
{"op":"editor","path":"/tmp/claude-prompt-1.md"}
{"taken":2}
```

When the window did not ask for the text it answers `"declined"`, and
`adroit editor` opens the file in `$ADROIT_USER_EDITOR` instead, the editor
adroit found when it started the session.
