# Read the console in a Neovim of its own

Status: Accepted

## Context

The console shows a session's terminal, and the only way to look back was to
scroll it. Moving a cursor over what a session wrote, searching it, selecting
and copying from it, as in Vim's terminal buffers, was not possible, and
adroit's users work by Vim's keys.

## Decision

What the console holds, its scrollback included, is read in a second embedded
Neovim, which shows it as a buffer that cannot be changed, with the terminal's
colours, and is drawn in the console's place while it is read. A key the
console does not bind starts reading, and the reader's yank goes to the
clipboard.

## Alternatives considered

- Write a Vim-like copy mode over the terminal grid in adroit. Every motion,
  text object, search and count would have to be made again, never as the
  user's own mappings and settings have them.
- Read the console in the prompt's Neovim, in a buffer of its own. Reading
  would take the prompt's window away from what is being written, and
  everything that fills or reads the prompt would have to tell the buffers
  apart.

## Consequences

- A second Neovim runs beside the prompt's, configured apart from it.
- What a session writes while the console is read reaches the reader a
  little late, as the reader is shown it again from the terminal.
- Only what the terminal keeps can be read: a program on the alternate
  screen has no scrolled-off lines.
