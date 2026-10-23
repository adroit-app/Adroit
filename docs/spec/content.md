# Content

The middle of the window is the content. It holds tabs, each tab is split into
windows, and each window shows one thing: a session, a session not started
yet, or a shell or Neovim adroit runs ([terminals](terminals.md)).

## Tabs

The tabs line up above the windows, numbered from 1. A tab is named after what
its focused window shows, with how many windows it holds, and keeps its own
focused window while another tab is shown. A tab holds whatever was opened in
it, from any worktree. Closing a tab closes every window in it.

## Windows

A window is split side by side or one above the other; the new window lies
right of it or below it, shows the same thing and takes the focus. Splits
nest. Windows are resized by keys or by dragging the border between them,
moved to a side of their tab, exchanged, rotated and moved to a tab of their
own ([keys](keys.md#actions)).

The same thing may stand in any number of windows and tabs. A program shown in
several windows runs at one size, the focused window's when it is one of them.

Each window has a strip on top that tells what it shows: its state, its name,
its agent, and the worktree it is in. The focused window is marked as the
prompt is when it has the keys. A session's window shows the agent's
interface; the focused one shows the prompt below it as well
([prompt](prompt.md)). A shell's or Neovim's window takes every key but the
window commands' prefix and `Mod` keys.

## Opening

Opening something, from the tree, the picker, the next waiting session or
anywhere else, moves the focus to the window of the shown tab that already
shows it. When none does, it is shown in the focused window, in place of what
that window showed, or in a new tab when nothing is open.

## Closing

Closing a window or a tab only hides what it shows: sessions, shells and
Neovim run on, and the tree still reaches them. A session not started yet
that no window shows any longer is given up.

A shell that exits or a session that ends closes every window that shows it.
When that leaves nothing open, the next session or terminal of its worktree is
shown.

## Across runs

What the focused window showed when the window closed is shown again, in a tab
of its own, when it opens; the other tabs and windows are not kept.

