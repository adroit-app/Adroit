# Split the content into tabs and windows as Neovim does

Status: Accepted

## Context

A tab showed one session or shell. Working on a session while watching a
shell, or reading a file it changed, took switching tabs back and forth. The
people adroit is for split windows in Neovim every day.

## Decision

The content holds tabs, and each tab is split into windows as Neovim splits
its tabs: side by side or one above the other, nested, resized and closed
freely. A window shows one thing, and the same thing may stand in any number
of windows and tabs. Closing a window hides what it shows and never ends a
program.

## Alternatives considered

- Keep one thing per tab. Two things could never be seen at once.
- Allow each thing in one window only, moving it when opened elsewhere.
  Opening a session in a new tab would empty the window it stood in, and the
  user could not keep a session in sight in two layouts.
- Lay windows out in a fixed grid. Neovim's users expect a split to divide the
  window they are in.

## Consequences

- Every place that opened something "in the console" has to say which window
  it opens in.
- A tree row can stand in several tabs, and shows which.
- Programs run independently of windows, as
  20261020-keep-terminal-resources-alive-independently-of-their-tabs.md
  already has them do of tabs.
