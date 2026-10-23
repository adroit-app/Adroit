# Read the console in a Neovim of its own

Status: Accepted

## Context

Looking back over what a session wrote meant scrolling its terminal. Moving a
cursor over it, searching, selecting and copying, as in Vim's terminal
buffers, was not possible, and adroit's users work by Vim's keys.

## Decision

What a console holds, its scrollback included, is read in a second Neovim the
window attaches to, as a buffer that cannot be changed, drawn in the console's
place while it is read.

## Alternatives considered

- A Vim-like copy mode written over the terminal grid. Every motion, text
  object and search would be made again, never as the user's mappings have
  them.
- Read it in the prompt's Neovim. Reading would take the prompt's window away
  from what is being written.

## Consequences

- A second Neovim runs beside the prompt's, configured apart from it.
- Only what the terminal keeps can be read: a program on the alternate screen
  has no scrolled-off lines.
- What is copied in it reaches the window's clipboard only through what Neovim
  tells its interface.
