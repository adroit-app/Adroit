<p align="center">
  <picture>
    <source media="(prefers-color-scheme: dark)" srcset="assets/logo/adroit-logo-horizontal-light.svg">
    <img src="assets/logo/adroit-logo-horizontal.svg" alt="Adroit" width="360">
  </picture>
</p>

# Adroit

A workspace for Claude Code, Codex, Pi and OpenCode sessions: their own terminal interfaces, prompts in Neovim, Vim-style moves.

> [!NOTE]
> adroit is early: it runs, but expect rough edges and changes.

One window holds your Claude Code, Codex, Pi and OpenCode sessions, grouped where
it runs. You start one by choosing a directory and talking; nothing has to be
named or declared first:

```text
api                              ← a workspace: a repository sessions run in
├── main                         ← a worktree
│   ├── auth-revamp              ← a Claude Code session
│   └── look-around              ← a session that only reads
└── feat/oauth
    ├── implement
    ├── $ dev-server             ← a named shell any session there can read
    └── $ test-watch
notes                            ← a directory outside git
└── outline-the-talk
```

The sessions waiting on you are listed first, across every workspace. Each
session shows its agent's real terminal interface; you write to it in an embedded
Neovim with your own configuration, and you move between panes with Vim's
window motions. See [docs/concept.md](docs/concept.md) for the ideas behind it.

## Install

Not yet released.

## Usage

adroit needs Claude Code (`claude`) or Codex CLI (`codex`), Neovim (`nvim`) and git on `PATH`, and jj
to read jj repositories through it. Build and run it from the development
shell:

```sh
nix develop -c cargo run --release -p adroit
```

On macOS, `just install-macos` builds it as the application Adroit and puts it in
`/Applications`.

It is signed by the self-signed certificate "Adroit Release" when the login
keychain has it, so a reinstall keeps the permissions macOS gave the previous
build; without it, the app is signed ad hoc and macOS asks again. Make the
certificate once, keep the folder, and import it on each Mac:

```sh
just macos-make-identity ~/adroit-identity
just macos-import-identity ~/adroit-identity
```

Press `w` to choose where a session works, from the directories sessions run
in, those Claude Code was used in, the repositories ghq keeps, or a file tree,
and say what you want done. adroit keeps no list of its own: the sessions are
the ones it runs, which keep running when the window closes
([details](docs/spec/tree.md)). `Mod-t` lists the past conversations of
Claude Code, Codex, Pi and OpenCode, to resume one in a new session
([details](docs/spec/sessions.md#resuming-a-past-conversation)). While they run, the Mac does not go to
idle sleep, so they go on with the display off
([details](docs/spec/sessions.md#keeping-the-mac-awake)).

The sessions run in two processes of adroit's own, a terminal host and a work
host, that keep running when the window closes; any number of windows can
show them, and New Window (`Cmd-Shift-N`) opens another
([details](docs/spec/sessions.md#windows)).

### Codex

Start a session as usual and choose Codex on its draft, by clicking it or with
`Ctrl-g a`; Settings → General → Agent sets which a draft starts with.
Settings → Agents → Codex configures its command and environment, with
session defaults for model, reasoning, approvals and sandbox. Each draft can
override its model and select a permission profile. Sessions already running keep their agent, so both can run together. From a terminal:

```sh
adroit session start --agent codex --dir ~/src/api/src -- "Review the API"
```

Sessions can also open sessions beside them and talk with each other
([details](docs/spec/opening-sessions.md)).

Read-only Codex sessions use its read-only sandbox. Recalling Codex's prompt
history in the embedded prompt is not yet supported. Use `Ctrl-g i` to operate
Codex's commands and menus directly; `Esc` returns to Adroit. See
[the session specification](docs/spec/sessions.md#agents) for the scope.

### Keys

The window has a tree, the console with the prompt below it, and a details
panel. `Ctrl-q` followed by `h` `j` `k` `l` moves between them, as `Ctrl-w`
does in Vim, and `w` `W` `p` `t` `b` move as Vim's do; the second key may be
pressed with `Ctrl` held too. `Ctrl-q` works everywhere, the prompt's insert
mode included, and `Ctrl-w` is left to the program: Neovim's windows, the
shell's word.

From anywhere, the prompt included, `Mod`, which is `Cmd` on macOS and
`Ctrl-Shift` elsewhere, runs the window's own commands: `Mod-]` `Mod-[` (or
`Ctrl-q Tab` `Ctrl-q Shift-Tab`) show the next or previous tab of the panel
the keys are in, the content's tabs or the details panel's overview, changes
and log, which `Mod-1` to `Mod-3` also show one by one; `Mod-p` chooses where
a new session works, `Mod-n` starts one where the console is, `Mod-s` opens a
shell there and `Mod-e` Neovim, `Mod-f` filters the tree, `Mod-r`
refreshes, `Mod-t` resumes a past conversation, `Mod-Backspace` ends one, and
`Mod-b` `Mod-d` show or hide the tree and the details panel. A scratch
session, which needs no directory chosen, starts from the picker's first row
or the Session menu.

| In the tree | |
| --- | --- |
| `j` `k`, `gg` `G` | move |
| `Enter` `o` | open a session, worktree or file: go to where it is already shown, in any tab, or else open it in a new tab, a file in Neovim or in a viewer by its kind; fold or unfold a workspace, a heading or a directory |
| `l` `Shift-Enter` `Ctrl-Enter` | look at it in the preview tab, with the keys staying in the tree; go into a workspace, a heading or a directory, to its first row |
| `s` `v` `t` | open it in a new split below or beside the focused window, or in a new tab though it is shown already |
| `h` | fold the row, or else the one it sits under, going there |
| `]w` `[w` | the next or previous session waiting on you, with the console ready to answer it; `Ctrl-q ]` `Ctrl-q [` from anywhere |
| `w` | choose where a new session works |
| `n` `$` `e` | start a session, a shell or Neovim in the selected worktree or directory |
| `x` | close the windows showing the selected row; what they show runs on |
| `y` | copy the path of a file or worktree, or the name of a session or shell |
| `O` | resume a past conversation ([details](docs/spec/sessions.md#resuming-a-past-conversation)) |
| `X` | end a session; Claude Code keeps its conversation, for `/resume` |
| `i` | write a prompt |
| `/` | narrow the tree to sessions by name; `Enter` keeps it, `Esc` clears it |
| `!` | list every worktree, those without sessions too, or only those with sessions; on a file, show or hide the hidden files |
| `r` | refresh |

In the console, a session takes only the keys its own controls need: `Enter`,
`←` `→`, `Tab` `Shift-Tab`, `F2`, and `Ctrl-l` `Ctrl-o` `Ctrl-t` `Ctrl-_`.
`↑` `↓` and `Ctrl-p` `Ctrl-n` recall the prompts you sent, into the prompt. `Ctrl-c` sends `Esc` and `g Ctrl-c` sends `Ctrl-c`; `Esc`
itself stays with adroit. `i` `a` `o` and the like start writing in the prompt.
Vim's keys, `j` `k`, `Ctrl-u`, `gg`, `/`, `v` and the rest, read the console in
your own Neovim: move, search, select, and `y` copies to the clipboard. `q`
goes back to the console live. The mouse wheel scrolls any console.
`Ctrl-g Ctrl-o` sends the next key as it is, and `Ctrl-g i` passes every key on
until `Esc`. While a session waits on you, at a permission prompt or a
question, every key but `Ctrl-q` sequences answers it as it is. Text that lands in
Claude Code's own input meanwhile moves into the prompt when the keys stop
going there, and whenever the prompt takes the keyboard. A shell takes every
key but `Ctrl-q` sequences; `Ctrl-q .` sends a literal `Ctrl-q`.

In the prompt, keys go to Neovim, `/` offers Claude Code's own commands, your
skills and plugins' included, `@` offers sessions and paths to walk into, a
line starting with `!` runs as a shell command in the session, `Tab` and
`Shift-Tab` go to the agent as they are, even while you write, and
`Ctrl-Enter` sends what you wrote. Starting a new session, `Ctrl-g a` and
`Ctrl-g A` switch the agent it will run.

In the details panel, `j` `k` and `gg` `G` move through a list and `Enter`
opens a row: a changed file shows its diff, a commit the files it changed. `h`
or `Esc` goes back, and in a diff `j` `k` scroll. A jj repository is read
through jj, its stack in the log ([details](docs/spec/details.md)).

`Cmd-+` `Cmd--` zoom the window in and out, and `Cmd-0` back. `Cmd-,` opens
the settings ([details](docs/spec/settings.md)).

These are adroit's own bindings. Change any of them, or add your own, in
`~/.config/adroit/keys.toml` ([format](docs/spec/keys.md)). The menu bar
offers every action that is more than a key, each with the Command key bound
to it.

### From a session

A session lists terminal resources with `adroit terminal list --json` and
reads one with `adroit terminal screen --terminal <resource-id>`
([details](docs/spec/content.md#terminals)). Programs can show images with the kitty
graphics protocol ([details](docs/spec/content.md#images-in-the-console)).

## Development

Enter the development shell with `nix develop` (or copy `.envrc.sample` to
`.envrc` and `direnv allow`), then:

```sh
just fmt   # format every source
just ci    # everything CI runs: format check, lint, tests
```

The test recipe and native daemon fixtures remove inherited Adroit assignment
variables, so checks can run from a session Adroit started without borrowing
its identity.
On macOS, a Codex workspace sandbox also needs temporary Unix sockets for the
native integration tests. From the development shell, run:

```sh
codex sandbox -P :workspace --allow-unix-socket /tmp --allow-unix-socket "$TMPDIR" -- just ci
```

The design lives in [docs/concept.md](docs/concept.md), the decisions behind it
in [docs/adr/](docs/adr/), and the behaviour it promises in
[docs/spec/](docs/spec/).

## License

MIT. See [LICENSE](LICENSE).
