# Put app commands on Cmd on macOS and on Ctrl+Shift elsewhere

Status: Accepted

## Context

adroit's app commands, such as closing a tab or opening the picker, sat on
Cmd. adroit is meant to run on Windows and Linux too, where there is no Cmd,
the Windows key belongs to the system, and plain Ctrl belongs to the
terminals adroit shows.

## Decision

App commands are bound to a modifier named `Mod`: Cmd on macOS, Ctrl+Shift
on Windows and Linux, as terminal applications there leave Ctrl to the
terminal and take Ctrl+Shift for themselves. Defaults use neither Alt nor
`Mod+Shift`.

## Alternatives considered

- Bind app commands to Ctrl on Windows and Linux. Shells and Neovim would lose
  keys they rely on.
- Bind them to Alt. Window managers and readline already use it.

## Consequences

- One set of default bindings serves every platform.
- `Mod+Shift` cannot be told apart from `Mod` on Windows and Linux, so a
  binding that needs it is the user's own choice.
- Copying and pasting move to `Mod` too, leaving `Ctrl+v` to Neovim on
  Windows and Linux.
