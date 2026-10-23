# Open a file from the tree in Neovim or in a viewer by its kind

Status: Accepted

## Context

The tree now lists a worktree's files. A text file is best read and changed
in the user's own Neovim, but images, movies, HTML pages and PDFs mean
nothing as text, and sessions make all of them.

## Decision

A file opened from the tree opens by its kind: a text file in Neovim, and an
image, a movie, an HTML page or a PDF in a viewer of its kind drawn by the
window itself. A viewer only reads its file. The key pressed says where it
opens; the file says what opens it.

## Alternatives considered

- Hand every file that is not text to the operating system's default
  application. The file would leave the window and the layout it was opened
  in.
- Open every file in Neovim. Images, movies and PDFs cannot be seen there.

## Consequences

- Viewers run in the window, not in the session daemon, and close with it.
- What decides a file's kind is adroit's, and a file of an unknown kind opens
  in Neovim.
