# Draw the file tree's icons from vim-nerdfont and vim-glyph-palette at build time

Status: Accepted

## Context

The file tree is meant to look as fern does with nerdfont in the user's
Neovim: a Nerd Font glyph for each kind of file, coloured by kind. Which
glyph a file gets and which colour a glyph takes are long tables that
vim-nerdfont and vim-glyph-palette already keep.

## Decision

adroit reads vim-nerdfont's glyph tables and vim-glyph-palette's default
palette at build time, from pinned revisions of both, and builds them into
the binary. A glyph is found as `nerdfont#find` finds it, and a palette
colour N is the theme's ANSI colour N, so icons change with the theme.
Glyphs are drawn in the user's own Nerd Font, which the terminal already
uses; adroit ships none.

## Alternatives considered

- Write adroit's own table of icons. It would fall behind the plugins the
  user already relies on in Neovim and look different from them.
- Read the tables from the user's Neovim at run time. The file tree would
  depend on plugins the user may not have installed.
- Ship a Nerd Font. The terminals already draw in the user's font, and a
  second font would make the tree's glyphs differ from theirs.

## Consequences

- Updating the icons is bumping a pinned revision.
- The two projects' licences ship beside adroit's.
- Without a Nerd Font, the glyphs show as missing characters.
