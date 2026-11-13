# Move a window into another tab, and place it there with the keys that move windows

Status: Accepted

## Context

Things opened from the tree come in tabs of their own, so two things wanted
side by side often end up in two tabs. A window could be moved out of its tab
into a new one, but not back into a tab that already exists; putting two tabs'
things together meant opening one of them again from the tree in a split and
closing the other tab. Neovim has no command for this; tmux has `join-pane`,
the inverse of the `break-pane` the move to a new tab already mirrors.

## Decision

A window can be moved into another tab, as the inverse of moving it into a
tab of its own. It is split off the window focused there and keeps the focus;
the tab it leaves closes when it held nothing else. Moving it says only which
tab it goes to: where it stands in that tab is then changed by the keys that
already move windows within a tab. The mouse does the same by dragging a
window onto a side of a window or onto a tab, or a tab onto a side of a window
for its focused window; a tab dragged onto a tab takes its place among the
tabs instead, as tabs are reordered in browsers. Dropped anywhere else,
nothing moves.

## Alternatives considered

- Merge whole tabs, every window of one into another. A tab is usually one
  window, and moving windows one at a time reaches every merge while keeping
  one gesture.
- Let the keys that move a window to a side of its tab cross into the next
  or previous tab. They would stop meaning "within this tab", as they do in
  Neovim, and reaching a tab further away would take several presses.
- Open the other tab's thing in a split from the tree and close its tab. It
  works today, but takes several steps and loses the window's size and
  focus.
- Let a window dropped in the middle of another take its place. Nothing
  opened replaces what a window shows, as recorded in
  `20261024-open-in-a-tab-of-its-own-and-look-in-one-preview-tab.md`.

## Consequences

- Moving a window into a tab takes the tab as well as the window.
- Dragging needs drop targets on the sides of windows and on the tabs, which
  the window draws while a drag is under way.
