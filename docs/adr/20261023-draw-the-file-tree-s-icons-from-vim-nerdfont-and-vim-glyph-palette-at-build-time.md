# Draw the file tree's icons from vim-nerdfont and vim-glyph-palette at build time

Status: Accepted

## Context

The file tree is meant to look as the user's Neovim file tree does with Nerd
Font icons. Which glyph a file gets and which colour it takes are long tables
that vim-nerdfont and vim-glyph-palette already keep.

The glyphs sit in Unicode's private use area, which only a Nerd Font draws.

## Decision

adroit builds both projects' tables into its binary at build time, from pinned
revisions, finds glyphs as vim-nerdfont does and colours them from the theme as
vim-glyph-palette does. The glyphs are drawn in the Nerd Font adroit ships,
as 20261023-ship-jetbrainsmono-nerd-font-mono-and-draw-the-console-in-it-by-default.md
decides.

## Alternatives considered

- adroit's own icon table. It would fall behind and look different from the
  user's Neovim.
- Read the tables from the user's Neovim at run time. The tree would depend on
  plugins the user may not have.
- Draw the glyphs in the user's own Nerd Font. Without one installed under a
  name the window can find, every icon shows as a missing character.

## Consequences

- Updating the icons is bumping a pinned revision.
- The two projects' licences ship beside adroit's.
- The icons look the same whatever the console's font, and may differ in shape
  from those the user's terminal draws.
