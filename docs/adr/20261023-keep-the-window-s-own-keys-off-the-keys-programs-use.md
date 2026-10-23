# Keep the window's own keys off the keys programs use

Status: Accepted

## Context

The window shows shells, Neovim and agents, which use plain Ctrl keys
heavily, `Ctrl-w` among them. adroit is meant to run on Linux and Windows as
well as macOS, where there is no Cmd and the system keeps the Windows key.
Alt belongs to window managers and readline.

## Decision

By default, app commands sit on a platform modifier: Cmd on macOS, Ctrl+Shift
elsewhere, as terminal applications there take Ctrl+Shift for themselves.
Window and tab commands follow a prefix, `Ctrl+q`, after which keys mean what
they mean after Vim's `Ctrl-w`. Every other Ctrl key reaches the program.

## Alternatives considered

- Window commands after `Ctrl+w`, as in Vim. Neovim in a window and a shell
  deleting a word lose the key, or the key means two programs' windows at once.
- App commands on plain Ctrl outside macOS. Shells and Neovim lose keys they
  rely on.
- Other prefixes: `Ctrl+Space` switches input sources on macOS, `Ctrl+\` is
  Vim's way out of terminal mode and awkward on JIS keyboards, `Ctrl+a` and
  `Ctrl+b` are in daily use by readline and Vim.

## Consequences

- One set of defaults serves every platform.
- Sending the prefix key to a program takes an explicit command.
- The window commands learned from Vim work after the prefix.
