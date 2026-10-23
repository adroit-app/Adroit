# Prefix window commands with Ctrl-q and leave Ctrl-w to the program

Status: Accepted

## Context

adroit's window commands began with `Ctrl+w`, as Vim's do. Neovim running in
an editor window uses `Ctrl-w` for its own windows, and a shell deletes the
last word with it, so one of the two always lost the key. Alt is taken by
window managers, and Cmd exists only on macOS.

## Decision

Window and tab commands begin with `Ctrl+q`, and the key after it means what
it means after Vim's `Ctrl-w`. `Ctrl+q` works in every area, the prompt's
insert mode included. `Ctrl+w` is bound nowhere by default and always reaches
the program.

## Alternatives considered

- Keep `Ctrl+w`, and hand it to Adroit only when Neovim's window is at an
  edge. Inside Neovim, `Ctrl+w v` or `Ctrl+w o` could mean either program's
  window.
- `Ctrl+Space`. macOS switches input sources with it.
- `Ctrl+\`. Vim leaves terminal mode with `Ctrl-\ Ctrl-n`, and it is hard to
  type on JIS keyboards.
- `Ctrl+a` or `Ctrl+b`, as tmux does. Readline and Vim use both often.
- `Ctrl+Shift+w`. It works, but `Ctrl+q` is easier to press.

## Consequences

- Programs no longer receive `Ctrl+q` directly; sending it takes an explicit
  command.
- Every window command learned from Vim works after the new prefix.
- The prefix is a binding like any other, so the user can move it.
