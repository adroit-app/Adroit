# Lay the window out as a tree, a content and a detail

Status: Accepted

## Context

The window was a sidebar, a console and a details panel. The console showed
one session or shell at a time, and the details panel described whatever the
console showed. Files, shells and editors are now worked on beside sessions,
several at once, so the middle of the window has to hold more than one thing,
and "what the console shows" stops naming a single thing.

## Decision

The window is a tree, a content and a detail, side by side. The tree lists
what there is to open. The content holds everything opened, in tabs and
windows. The detail describes the thing shown by the content's focused
window, and follows the focus as it moves.

## Alternatives considered

- Keep the console and open files in a separate panel. Two places would show
  opened things, each with its own way of moving between them.
- Let the detail describe the row selected in the tree. Moving the cursor
  through the tree would change the detail while the user works in a window
  the detail no longer describes.

## Consequences

- The console becomes one kind of window in the content.
- With nothing focused in the content, the detail has nothing to describe.
- Moving the focus between windows changes the detail.
