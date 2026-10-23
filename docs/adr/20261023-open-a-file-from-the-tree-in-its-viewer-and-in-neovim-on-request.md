# Open a file from the tree in its viewer, and in Neovim on request

Status: Proposed

## Context

The tree lists a worktree's files. Sessions make images, movies, pages and
documents that mean nothing as text, and Markdown they write is read far more
often than it is changed. Changing a file is still best done in the user's own
Neovim.

## Decision

A file of a kind adroit has a viewer for, Markdown among them, opens in that
viewer, drawn by the window itself. Any file, whatever its kind, opens in
Neovim through an action of its own. A viewer only reads its file.

## Alternatives considered

- Open text, Markdown included, in Neovim, and only media and documents in a
  viewer. Reading what a session wrote would mean reading its markup.
- Hand non-text files to the operating system's default application. The file
  would leave the window and its layout.
- Open every file in Neovim. Images, movies and documents cannot be seen there.

## Consequences

- Viewers run in the window, not in the hosts, and close with it.
- A file of a kind adroit does not know opens in Neovim either way.
- A file with a viewer takes one more step to change.
