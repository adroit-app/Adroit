# adroit

A workspace where you work with many Claude Code sessions by hand: the Claude TUI as it is, prompts in your own Neovim, Vim-style moves.

## How it is built

- **Test first.** Red → green → refactor. Write the failing test before the
  code that makes it pass.
- **Each layer holds one thing.** Put a fact in the one layer that owns it,
  and nowhere else.

  | Layer | Holds |
  | --- | --- |
  | `docs/concept.md` | what this is and for whom |
  | `docs/adr/` | one decision and why it was made |
  | `docs/spec/` | the behaviour users and other components can rely on |
  | Tests | the details that pin the behaviour down: rules, boundaries, tables |
  | Code | how it is done |
  | Commits | why a change was made |
  | Comments | why it is not done another way |

  Before writing, fixing or reviewing a record, read `docs/adr/README.md`;
  before writing, fixing or reviewing a spec, read `docs/spec/README.md`.
  Follow their writing order.
- **Dependencies are current.** Before adding or bumping one, look up its
  latest release in the registry. Do not copy a version from memory or from
  another repository.

## Quality gate

Run `just ci` before reporting a change as done. CI runs the same recipe.

## Layout

| Path | What lives there |
| --- | --- |
| `crates/adroit/` | the `adroit` application binary: the window's Dioxus UI, its menu bar and Settings window |
| `crates/adroit/assets/` | the windows' stylesheets and page scripts, taken in with `include_str!` |
| `crates/agents/` | each agent's adapter: how Claude Code and Codex are started, asked and read back |
| `crates/agents/plugin/` | the Claude Code plugin every session adroit starts loads |
| `crates/core/` | the window's state and the rules it changes by, with no UI and no programs run |
| `crates/core/icons/` | the Tabler icons the window draws, with their licence |
| `crates/protocol/` | what the window and the work host say to each other: requests, events, the work they describe, the versions they speak |
| `crates/host/` | what the window runs: the daemon, sessions, shells, editor, socket, delegation and what is kept on disk |
| `crates/host/themes/` | the console themes adroit comes with |
| `crates/host/terminfo/` | the terminfo entry the console's programs are given |
| `crates/host/tests/` | tests that start the daemon, sessions and shells |
| `assets/logo/` | the logo as flattened SVG, and `build.py` that regenerates it |
| `assets/macos/` | the macOS app's Info.plist and the script that assembles the app |
| `assets/file-icons/` | vim-nerdfont's and vim-glyph-palette's icon tables at a pinned revision, their licences, and `update.sh` that copies them again |
| `assets/fonts/` | JetBrainsMono Nerd Font Mono in the styles the console draws, its licences, the release it came from, and `update.sh` that copies it again |
| `docs/` | concept, ADRs, specs, references |

## Language

- README, docs and commit messages: English
- Code comments, log and error messages: English

## Commits

Scoped Commits:

```
<scope>: <description>

<why this change is needed>
```

- The scope names where the change is, from the table below. Several scopes
  are joined with `, `; a change across the whole tree uses `treewide`.
- The description starts in lower case, has no trailing period, and stays
  within 72 characters.
- The body explains why. The code shows how; the tests show what.
- A release commit is `release: vX.Y.Z`.
- A breaking change says so in the body and carries a `BREAKING CHANGE:`
  trailer.

| Scope | Covers |
| --- | --- |
| `adroit` | `crates/adroit/` |
| `agents` | `crates/agents/` |
| `core` | `crates/core/` |
| `host` | `crates/host/` |
| `nvim` | `crates/nvim/` |
| `model` | `crates/model/` |
| `protocol` | `crates/protocol/` |
| `term` | `crates/term/` |
| `vcs` | `crates/vcs/` |
| `assets` | `assets/` |
| `docs` | `docs/concept.md`, `README.md`, `AGENTS.md` |
| `adr` | `docs/adr/` |
| `spec` | `docs/spec/` |
| `ci` | `.github/` |
| `nix` | `flake.nix`, `flake.lock` |
| `tooling` | `justfile`, `rust-toolchain.toml`, `Cargo.toml`, `clippy.toml`, `.envrc.sample`, `.gitattributes` |
| `deps` | dependency updates |

Add a row when a new area appears.
