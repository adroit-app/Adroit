# Bind the window's own keys as each platform does, off the keys programs use

Status: Accepted

## Context

The window shows shells, Neovim and agents, which use plain Ctrl keys
heavily, `Ctrl-w` among them. Adroit is meant to run on Linux and Windows as
well as macOS, where there is no Cmd and the system keeps the Windows key.
Alt belongs to window managers and readline. Users come with the keys other
applications taught them: Command or Ctrl with Shift for an application's
commands, and keys browsers, terminals and Vim switch tabs with.

## Decision

By default, app commands sit on a platform modifier: Cmd on macOS, Ctrl+Shift
elsewhere, as terminal applications there take Ctrl+Shift for themselves. A
command an application on that platform also has, such as a new tab, a new
window or a numbered tab, takes the key it has there.

Window and tab commands follow a prefix, `Ctrl+q`, after which keys mean what
they mean after Vim's `Ctrl-w`, and a key held with Ctrl means what it means
without. The one exception is `q`: as the prefix repeated is the quickest
move to the next window, `q` is that move with Ctrl held or not, and closing
a window is `c` alone, so how long Ctrl is held never decides between moving
and closing.

Every other Ctrl key reaches the program, but for the keys every platform's
browsers and terminals switch tabs with, which no program shown relies on:
those switch Adroit's tabs, in the prompt's insert mode too.

## Alternatives considered

- Window commands after `Ctrl+w`, as in Vim. Neovim in a window and a shell
  deleting a word lose the key, or the key means two programs' windows at once.
- App commands on plain Ctrl outside macOS. Shells and Neovim lose keys they
  rely on.
- Other prefixes: `Ctrl+Space` switches input sources on macOS, `Ctrl+\` is
  Vim's way out of terminal mode and awkward on JIS keyboards, `Ctrl+a` and
  `Ctrl+b` are in daily use by readline and Vim.
- `q` closing as in Vim, with the prefix repeated moving on. The prefix
  pressed twice then moves or closes by whether Ctrl was let go in between.
- Tab switching only after the prefix. Each switch takes two keys, and the
  keys users already know switch nothing.

## Consequences

- One set of defaults serves every platform, each with its own conventions.
- Sending the prefix key to a program takes an explicit command.
- The window commands learned from Vim work after the prefix, but for `q`.
- A program in the console never receives the tab switching keys.
