# Merge a tab into another whole, and move a single window by dragging it

Status: Accepted

## Context

Things opened from the tree come in tabs of their own, so two things wanted
side by side often end up in two tabs. A window could be moved out of its tab
into a new one, but nothing brought tabs back together: putting them side by
side meant opening one again from the tree in a split and closing the other
tab. Neovim has no command for this either.

## Decision

A tab can be merged into another: every window of it, split as it was, comes
beside the windows of the other, and the tab merged closes. The keys merge
the tab shown, naming the tab it goes into; one window is merged on its own by
first moving it into a tab of its own. Dragged with the mouse, a window's strip
moves that window alone, onto a side of a window or into a tab, and a tab
moves all its windows onto a side of a window. Dropped anywhere else, nothing
moves.

## Alternatives considered

- Move only the focused window into the other tab, as the inverse of moving
  it into a tab of its own. Merging a tab split in two took a key per window,
  and the split it had was lost.
- Let the keys that move a window to a side of its tab cross into the next
  or previous tab. They would stop meaning "within this tab", as they do in
  Neovim, and reaching a tab further away would take several presses.
- Let a window dropped in the middle of another take its place. Nothing
  opened replaces what a window shows, as recorded in
  `20261024-open-in-a-tab-of-its-own-and-look-in-one-preview-tab.md`.

## Consequences

- Merging takes the tab it goes into as well as the tab merged.
- Dragging needs drop targets on the sides of windows and on the tabs, which
  the window draws while a drag is under way.
