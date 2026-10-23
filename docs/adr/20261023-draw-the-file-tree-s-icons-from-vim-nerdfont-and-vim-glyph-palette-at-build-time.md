# Draw the file tree's icons from vim-nerdfont and vim-glyph-palette at build time

Status: Accepted

## Context

The file tree is meant to look as the user's Neovim file tree does with Nerd
Font icons. Which glyph a file gets and which colour it takes are long tables
that vim-nerdfont and vim-glyph-palette already keep.

## Decision

adroit builds both projects' tables into its binary at build time, from pinned
revisions, finds glyphs as vim-nerdfont does and colours them from the theme as
vim-glyph-palette does. Glyphs are drawn in the user's own Nerd Font; adroit
ships none.

## Alternatives considered

- adroit's own icon table. It would fall behind and look different from the
  user's Neovim.
- Read the tables from the user's Neovim at run time. The tree would depend on
  plugins the user may not have.
- Ship a Nerd Font. The terminals already draw in the user's font.

## Consequences

- Updating the icons is bumping a pinned revision.
- The two projects' licences ship beside adroit's.
- Without a Nerd Font, the glyphs show as missing characters.
