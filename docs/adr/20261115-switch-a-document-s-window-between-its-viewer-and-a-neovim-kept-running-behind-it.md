# Switch a document's window between its viewer and a Neovim kept running behind it

Status: Accepted

## Context

A Markdown document is read and written in turn: read as it is drawn, then
corrected in Neovim, then read again. Opening what a window does not show
never takes the place of what a window shows, so reaching the Neovim meant
another window or another tab, and a Neovim no window shows any longer is
asked to quit, losing its undo history and its place in the file.

## Decision

A window that shows a document's viewer switches, in place, to a Neovim on the
document, and back. The Neovim the window switches away from keeps running,
hidden behind the viewer, for as long as that window shows the document's
viewer; switching back shows the same Neovim. Once the window closes, the
Neovim is asked to quit as any Neovim no window shows is.

## Alternatives considered

- **Opening the Neovim in a split or a tab of its own.** The document and its
  source would take two windows where one does, and the reader would move
  between windows for what is one task.
- **Letting the hidden Neovim quit, and starting one again each time.** Undo
  history, the cursor and the time to start would be lost at every switch, and
  a Neovim still quitting could be switched to and end under the window.
- **Showing the viewer and Neovim side by side, always.** Most of a document's
  life is spent being read, and the source would halve the room it is read in.

## Consequences

- One window can stand for a running program it does not draw, which the
  rule that a hidden Neovim quits has to except.
- Switching is the one way something takes the place of what a window shows
  other than the preview.
- A Neovim is started for a window that may show something else by the time
  it runs; it then opens as anything else opens.
