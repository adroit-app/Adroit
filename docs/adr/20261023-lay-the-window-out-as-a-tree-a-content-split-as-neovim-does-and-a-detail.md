# Lay the window out as a tree, a content split as Neovim does, and a detail

Status: Accepted

## Context

Files, shells and editors are worked on beside sessions, several at once. A
single console showing one thing at a time made watching one while working on
another a matter of switching back and forth. The people adroit is for split
windows in Neovim every day.

## Decision

The window is a tree of what can be opened, a content holding what is open,
and a detail describing what the content's focused window shows. The content
holds tabs split into windows as Neovim splits them. A window shows one thing,
the same thing may stand in any number of windows, and closing a window never
ends a program: programs live apart from the windows that show them. The one
exception, a Neovim started on a file that loses nothing by quitting, is
recorded in
`20261024-let-a-file-s-row-stand-for-its-neovim-and-quit-it-as-qa-does-once-no-window-shows-it.md`.

## Alternatives considered

- Keep one console, and open other things in a panel of their own. Two places
  would show opened things, each moved through differently.
- Let the detail describe the row selected in the tree. It would change while
  the user works in a window it no longer describes.
- One thing per tab, or each thing in one window only. Two things could not be
  seen together, or kept in sight in two layouts.
- End a program when its tab or window closes. A view change would destroy
  what other work still uses.

## Consequences

- Opening something has to say which window it opens in.
- Moving the focus changes the detail; with nothing focused it is empty.
