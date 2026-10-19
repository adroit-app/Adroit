<p align="center">
  <picture>
    <source media="(prefers-color-scheme: dark)" srcset="assets/logo/adroit-logo-horizontal-light.svg">
    <img src="assets/logo/adroit-logo-horizontal.svg" alt="adroit" width="360">
  </picture>
</p>

# adroit

A workspace where you work with many Claude Code sessions by hand: the Claude TUI as it is, prompts in your own Neovim, Vim-style moves.

> [!NOTE]
> adroit is early: it runs, but expect rough edges and changes.

One window holds every Claude Code session you are working with, found where
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
session shows the real Claude Code interface; you write to it in an embedded
Neovim with your own configuration, and you move between panes with Vim's
window motions. See [docs/concept.md](docs/concept.md) for the ideas behind it.

## Install

Not yet released.

## Usage

adroit needs Claude Code (`claude`), Neovim (`nvim`) and git on `PATH`, and jj
to read jj repositories through it. Build and run it from the development
shell:

```sh
nix develop -c cargo run --release -p adroit
```

On macOS, `just install-macos` builds it as the application Adroit and puts it in
`/Applications`.

Press `w` to choose where a session works, from the directories sessions run
in, those Claude Code was used in, the repositories ghq keeps, or a file tree,
and say what you want done. adroit keeps no list of its own: the sessions are
the ones it runs, which keep running when the window closes, and the ones
Claude Code reports running in other terminals
([details](docs/spec/workspaces.md)). While they run, the Mac does not go to
idle sleep, so they go on with the display off
([details](docs/spec/sessions.md#keeping-the-mac-awake)).

### Keys

The window has a sidebar, the console with the prompt below it, and a details
panel. `Ctrl-w` followed by `h` `j` `k` `l` moves between them, as in Vim.

From anywhere, the prompt included: `Cmd-]` `Cmd-[` (or `Ctrl-w n` `Ctrl-w p`)
show the next or previous tab of the panel the keys are in, the console's
tabs or the details panel's overview, changes and log, which `Cmd-1` to
`Cmd-3` also show one by one; `Cmd-o` chooses where a new session
works, `Cmd-n` starts one where the console is (`Cmd-Shift-n` a reader,
`Cmd-Option-n` a scratch session that needs no directory chosen),
`Cmd-s` opens a shell there, `Cmd-f` filters the sidebar, `Cmd-e` lists every
worktree, `Cmd-r` refreshes, `Cmd-t` takes over a session and
`Cmd-Backspace` ends one.

| In the sidebar | |
| --- | --- |
| `j` `k`, `gg` `G` | move |
| `Enter` `l` `o` | open a session or worktree; fold a workspace |
| `L` `Shift-Enter` `Ctrl-Enter` | show a session or worktree in the console, with the keys staying in the sidebar |
| `h` | fold |
| `]w` `[w` | the next or previous session waiting on you, with the console ready to answer it; `Ctrl-w ]` `Ctrl-w [` from anywhere |
| `w` | choose where a new session works |
| `n` | start a session in the selected worktree or directory |
| `N` | start a session that only reads |
| `s` `x` | open a shell in the worktree; close the current tab, ending a shell or putting a session back to the sidebar only |
| `t` | take over an interactive session ([details](docs/spec/interactive-sessions.md)) |
| `X` | end a session; Claude Code keeps its conversation, for `/resume` |
| `i` | write a prompt |
| `/` | narrow the sidebar to sessions by name; `Enter` keeps it, `Esc` clears it |
| `!` | list every worktree, those without sessions too, or only those with sessions |
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
question, every key but `Ctrl-w` answers it as it is. Text that lands in
Claude Code's own input meanwhile moves into the prompt when the keys stop
going there, and whenever the prompt takes the keyboard. A shell takes every
key; `Ctrl-w .` sends a literal `Ctrl-w`.

In the prompt, keys go to Neovim, `/` offers Claude Code's own commands, your
skills and plugins' included, `@` offers sessions and paths to walk into, a
line starting with `!` runs as a shell command in the session, and
`Ctrl-Enter` sends what you wrote.

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

A session reads the shells next to it with
`adroit shell list` and `adroit shell screen <name>`
([details](docs/spec/shells.md)). Programs can show images with the kitty
graphics protocol ([details](docs/spec/images.md)).

## Development

Enter the development shell with `nix develop` (or copy `.envrc.sample` to
`.envrc` and `direnv allow`), then:

```sh
just fmt   # format every source
just ci    # everything CI runs: format check, lint, tests
```

The design lives in [docs/concept.md](docs/concept.md), the decisions behind it
in [docs/adr/](docs/adr/), and the behaviour it promises in
[docs/spec/](docs/spec/).

## License

MIT. See [LICENSE](LICENSE).
