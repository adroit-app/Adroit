# Open a file from the tree in Neovim or in a viewer by its kind

Status: Accepted

## Context

The tree lists a worktree's files. Text is best read and changed in the user's
own Neovim, but sessions also make images, movies, pages and documents that
mean nothing as text.

## Decision

A file opens by its kind: text in Neovim, and media and documents in a viewer
of their kind drawn by the window itself. A viewer only reads its file.

## Alternatives considered

- Hand non-text files to the operating system's default application. The file
  would leave the window and its layout.
- Open every file in Neovim. Images, movies and documents cannot be seen there.

## Consequences

- Viewers run in the window, not in the session daemon, and close with it.
- A file of a kind adroit does not know opens in Neovim.
