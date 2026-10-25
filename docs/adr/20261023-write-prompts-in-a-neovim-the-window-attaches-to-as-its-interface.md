# Write prompts in a Neovim the window attaches to as its interface

Status: Proposed

## Context

A prompt to Claude Code is often long and edited several times before it is
sent, and the people adroit is for write everything in Neovim, with motions,
plugins and completion of their own. Claude Code's input line edits text with a
handful of keys, and a text field in the window would have no more. The
programs adroit runs, and the files a prompt names, are on the work host's
machine, which need not be the window's.

## Decision

The prompt is a Neovim running on the work host's machine, configured for
writing prompts apart from the user's everyday Neovim. The window attaches to
it as its user interface and draws it from the grid and the completion menu
Neovim reports.

## Alternatives considered

- A text field with Vim keys emulated in the window. Only the emulated part of
  Vim would work, never the user's own mappings and plugins.
- The user's Neovim run in a terminal in the window. Its completion menu would
  be limited to the cell grid, and adroit could neither fill nor read the text
  it holds without driving Neovim through keys.
- A Neovim on the window's machine. Its completion and file names would see
  another machine than the sessions, and text taken back from a session would
  have to travel to it.
- Claude Code's own external editor, opened for each prompt. Every prompt
  would start and leave an editor, and nothing could be written while the
  session works.

## Consequences

- A Neovim must be on the work host's machine, and the prompt starts with it.
- The work host reads and fills the prompt's text through Neovim's API, so
  sending, keeping a tab's text and recalling history do not depend on keys or
  on the window.
- A configuration the user writes for their everyday Neovim does not apply to
  the prompt unless they bring it over to the work host's machine.
- Every key and redraw of the prompt crosses between the machines.
