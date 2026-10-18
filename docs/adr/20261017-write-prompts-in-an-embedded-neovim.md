# Write prompts in an embedded Neovim

Status: Accepted

## Context

A prompt to Claude Code is often long and edited several times before it is
sent, and the people adroit is for write everything in Neovim, with motions,
plugins and completion of their own. Claude Code's input line edits text with a
handful of keys, and a text field in the window would have no more.

## Decision

The prompt is a Neovim that adroit embeds and draws itself from the grid and
the completion menu Neovim reports, configured for writing prompts apart from
the user's everyday Neovim.

## Alternatives considered

- A text field with Vim keys emulated in the window. Only the emulated part of
  Vim would work, never the user's own mappings and plugins.
- The user's Neovim run in a terminal in the window. Its completion menu would
  be limited to the cell grid, and adroit could neither fill nor read the text
  it holds without driving Neovim through keys.
- Claude Code's own external editor, opened for each prompt. Every prompt
  would start and leave an editor, and nothing could be written while the
  session works.

## Consequences

- Neovim must be installed, and the prompt starts with it.
- adroit reads and fills the prompt's text through Neovim's API, so sending,
  keeping a tab's text and recalling history do not depend on keys.
- A configuration the user writes for their everyday Neovim does not apply to
  the prompt unless they bring it over.
