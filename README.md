<p align="center">
  <picture>
    <source media="(prefers-color-scheme: dark)" srcset="assets/logo/adroit-logo-horizontal-light.svg?v=6770c89">
    <img src="assets/logo/adroit-logo-horizontal.svg?v=6770c89" alt="Adroit" width="360">
  </picture>
</p>

# Adroit

An Agent Workspace for people who live in Vim: drive any coding agent in its own
terminal interface, by hand and the way you edit.

> [!NOTE]
> Adroit is early: it runs, but expect rough edges and changes.

Claude Code, Codex, Pi and OpenCode sessions stand side by side in one window,
each in the interface its agent ships, unchanged. You write prompts in a real
Neovim, configured for Adroit with the mappings and plugins you choose, and
move between tabs, splits, the tree and the details panel with Vim's motions.
Sessions are grouped by workspace and worktree, across repositories, with
those waiting on you listed first.

```text
api                              ← a repository
├── main                         ← a worktree
│   ├── auth-revamp              ← a Claude Code session
│   ├── review-api               ← a Codex session
│   └── $ dev-server             ← a named shell
└── feat/oauth
    └── implement
notes                            ← a directory outside git
└── outline-the-talk
```

Sessions and shells keep running when the window closes. Open Adroit again to
return to them, or open several windows onto the same work. The details panel
shows the session's overview, changes, history, agents and resources.

```text
┌──────────────┬──────────────────────────────────┬─────────────┐
│ tree         │ 1 auth-revamp │ 2 review-api     │ details     │
│              ├────────────────┬─────────────────┤             │
│ api          │ the agent's    │ another window  │ Overview    │
│ ├── main     │ own interface  │ of the same tab │ Changes     │
│ │   ├── …    │                │                 │ Log         │
│ └── …        ├────────────────┤                 │ Agents      │
│              │ prompt: Neovim │                 │ Resources   │
│ waiting on   │                │                 │             │
│ you          │                │                 │ usage       │
└──────────────┴────────────────┴─────────────────┴─────────────┘
```

See [the concept](docs/concept.md) for who Adroit is for and the ideas behind it.

## Build and install

Adroit needs git and at least one supported agent on `PATH`. The macOS app
carries the Neovim its prompt and console reader run; built and run from the
source, they run `nvim` on `PATH`, or the program `ADROIT_NVIM` names. Opening a
file in Neovim runs the `nvim` on `PATH` either way.

| Agent       | Program                                               |
| ----------- | ----------------------------------------------------- |
| Claude Code | `claude`                                              |
| Codex CLI   | `codex`                                               |
| Pi          | `pi`                                                  |
| OpenCode    | `opencode` from `@opencode/cli`, version 2.1 or later |

Install jj to read jj repositories and create jj workspaces. ghq is optional;
when installed, its repositories are offered in the directory picker.

Build and run from the Nix development shell:

```sh
nix develop -c cargo run --release -p adroit
```

On macOS, build and install the application into `/Applications`:

```sh
nix develop -c just install-macos
```

The installed command is `/Applications/Adroit.app/Contents/MacOS/adroit`. Add
that directory to `PATH` to use the `adroit` commands below from your terminal.

The app is signed by the self-signed identity "Adroit Release" when the login
keychain has it, so reinstalling keeps the permissions macOS gave the previous
build. Otherwise it is signed ad hoc. To create and import the identity once,
keep the resulting folder and run these from the development shell:

```sh
just macos-make-identity ~/adroit-identity
just macos-import-identity ~/adroit-identity
```

## Start a session

1. Press `Mod+p` (`w` in the tree) to choose a directory. The picker offers
   workspaces already in use, Claude Code projects, ghq repositories and a file
   tree. Choose its scratch-session row, or **Session → New Scratch Session**, to
   start without choosing a directory.
2. On the draft, choose a profile, model and permissions. `Ctrl+g p` and
   `Ctrl+g P` move through profiles.
3. Write the first prompt and press `Ctrl+Enter`. It starts the session;
   later prompts go to that same session.

A first line such as `name: review-api` names the session and is removed from
the prompt sent to the agent. Without one, Adroit makes a name from the prompt.

To start a Codex session from a terminal:

```sh
adroit session start --agent codex --dir ~/src/api -- "Review the API"
```

To return to a conversation an agent kept, press `Mod+o` (`O` in the tree).
The list includes conversations from the logins your profiles use; choosing one
resumes it in that same login. See [sessions](docs/spec/sessions.md) and
[the directory picker](docs/spec/tree.md#going-to-a-session-or-starting-one).

### Profiles and logins

**Settings → Agents** holds named profiles. Each chooses an agent, its login,
launch command, environment and defaults for new sessions. You can keep several
profiles for one agent, and choose which profile new sessions start from.

A profile can use your terminal's login or keep a login of its own. The latter
has its own credentials, history and settings. Log in from the profile's page in
Settings. Existing sessions keep the profile and login they started with, even
when settings change.

Drafts can override their profile's model and permissions. Read-only and custom
permissions use the selected agent's own controls; their scope differs by agent.
See [profiles](docs/spec/settings.md#profiles) and
[agent behavior](docs/spec/sessions.md#agents).

## Move around

`Mod` means `Cmd` on macOS and `Ctrl+Shift` elsewhere. `Ctrl+q` starts window
motions, including while the prompt is in insert mode; `Ctrl+w` stays with the
program in the console or prompt.

| Keys                         | Action                                                           |
| ---------------------------- | ---------------------------------------------------------------- |
| `Ctrl+q h/j/k/l`             | Move between the tree, content windows, prompt and details panel |
| `Ctrl+q s`, `Ctrl+q v`       | Split below or beside the focused window                         |
| `Ctrl+Tab`, `Ctrl+Shift+Tab` | Next or previous tab; in details, next or previous sheet         |
| `Mod+1` … `Mod+9`            | Go to a content tab                                              |
| `Mod+p`                      | Choose where a session works                                     |
| `Mod+t`                      | Start a session in the selected or focused directory             |
| `Mod+o`                      | Resume a past conversation                                       |
| `Mod+n`                      | Open another Adroit window                                       |
| `Mod+s`, `Mod+e`             | Open a shell or Neovim in the current directory                  |
| `Mod+j`                      | Go to the next session waiting on you                            |
| `Mod+f`, `Mod+r`             | Filter the tree or refresh                                       |
| `Mod+b`, `Mod+d`             | Show or hide the tree or details panel                           |
| `Mod+,`                      | Open Settings                                                    |
| `F1`                         | Show the keys for the current area                               |

### Tree and files

Use `j`/`k` and `gg`/`G` to move. `Enter` or `o` opens a row, returning to a
window already showing it or opening a new tab. `l` previews it while the keys
stay in the tree. `s`, `v` and `t` open it in a split below, a split beside or
a new tab. Directories and headings fold and unfold; `h` goes back out.

Files open in Neovim or a viewer for their kind. `x` closes what a row shows
without ending its session or shell. `n`, `$` and `e` start a session, shell
or Neovim in the selected directory.

### Console and prompt

In a session's console, `i` starts writing in the prompt. Vim motions, search
and selection read the console through Neovim; `y` copies and `q` returns to the
live console. `Ctrl+g i` passes keys directly to the agent until `Esc`.
When an agent waits on a choice or permission, its console takes the answer
directly. A shell takes input directly too.

The prompt uses Neovim. `Ctrl+Enter` sends it; unsent text and attachments stay
with the session or draft as you switch tabs. Paste or drop images to attach
them. `@` completes sessions and paths, and `/` offers the selected agent's
commands and skills where supported. On an empty prompt, `Tab` can accept the
agent's suggestion; with text in the prompt, it is Neovim's.

See [the prompt](docs/spec/prompt.md), [key configuration](docs/spec/keys.md)
and the [complete key reference](docs/spec/reference/keys.md) for the controls
and agent-specific behavior. Bindings can also be changed in Settings.

## Sessions working alongside each other

Every session Adroit starts has the `adroit` MCP tools: `sessions`, `models`,
`start_session` and `send`. It can find the sessions already working in a
repository, ask what they know, and open a separate line of work in another
session, optionally in a new worktree.

Each session answers to you. Work that belongs to it stays with it and its
agent's own subagents; another Adroit session is a peer you can read and steer
directly. See [sessions that open sessions](docs/spec/opening-sessions.md).

Sessions can inspect named terminal resources with
`adroit terminal list --json` and
`adroit terminal screen --terminal <resource-id>`. A session's programs can
show images through the kitty graphics protocol. See
[terminals and console content](docs/spec/content.md).

To keep separate groups of sessions, use `adroit --instance <name>`. Each
instance has its own hosts and sessions; settings and keys are shared.
`adroit --new-window` opens another window of the current instance.

## Development

Enter the development shell with `nix develop` (or copy `.envrc.sample` to
`.envrc` and run `direnv allow`), then:

```sh
just fmt   # format every source
just ci    # format check, lint and tests, as in CI
```

For a machine that only runs the hosts and session commands, build without the
desktop window:

```sh
nix develop -c cargo build --release -p adroit --no-default-features
```

That executable serves `adroit host` and `adroit daemon` and runs the session,
terminal and MCP commands. It refuses requests to open a window or take editor
input. On the work machine it reaches that machine's native sockets.
`just test-headless` checks this build, including a terminal kept through replacing
its work host.

To use a window on another Mac, run the terminal host, the work host and the
gateway as services on the work machine. `adroit remote enable` registers all
three as the user's services, with the PATH the login shell has. The service
environment must find the agents, git, jj and any programs its sessions use.
On macOS, run it with the `adroit` inside Adroit.app. Replacing the work host
leaves terminals running; replacing the terminal host ends its programs.

Pair the client with the work machine: run `adroit connect pair <address>` on
the client, then `adroit remote pair` on the work machine and type the code the
client shows. The client adds an environment for the work machine, which every
window lists; check its connection with `adroit connect` first if you like.
Adding an environment from below the main window's tree pairs too, listing the
work machines announcing themselves on the local network.
See [remote hosts](docs/spec/remote-execution.md) for the configuration, recovery
workflow and the remote window's restrictions.

The tests isolate their state and remove inherited Adroit assignment variables,
so they can run from an Adroit session. Native integration tests need permission
to create temporary Unix sockets.

The [concept](docs/concept.md) explains the product,
[ADRs](docs/adr/) record decisions, and
[specifications](docs/spec/) describe the behavior. The generated
[key](docs/spec/reference/keys.md) and
[settings](docs/spec/reference/settings.md) references come from the code;
regenerate them with `just references` when changing their definitions.

## License

MIT. See [LICENSE](LICENSE).
