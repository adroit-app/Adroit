# Draw the file tree's icons from vim-nerdfont and vim-glyph-palette at build time

Status: Accepted; superseded in part, for the font the glyphs are drawn
from, by 20261023-ship-jetbrainsmono-nerd-font-mono-and-draw-the-console-in-it-by-default.md

## Context

The file tree is meant to look as fern does with nerdfont in the user's
Neovim: a Nerd Font glyph for each kind of file, coloured by kind. Which
glyph a file gets and which colour a glyph takes are long tables that
vim-nerdfont and vim-glyph-palette already keep.

The glyphs sit in Unicode's private use area, which only a Nerd Font draws.
The user's terminal may draw them from a font of its own that the window
cannot see, and the console's font in adroit need not be a Nerd Font at all.

## Decision

adroit reads vim-nerdfont's glyph tables and vim-glyph-palette's default
palette at build time, from pinned revisions of both, and builds them into
the binary. A glyph is found as `nerdfont#find` finds it, and a palette
colour N is the theme's ANSI colour N, so icons change with the theme.

adroit ships Nerd Fonts' symbols font, built into the binary, and draws the
glyphs from it whatever font the user has.

## Alternatives considered

- Write adroit's own table of icons. It would fall behind the plugins the
  user already relies on in Neovim and look different from them.
- Read the tables from the user's Neovim at run time. The file tree would
  depend on plugins the user may not have installed.
- Draw the glyphs in the user's own Nerd Font. Without one installed under a
  name the window can find, every icon shows as a missing character.

## Consequences

- Updating the icons is bumping a pinned revision.
- The licences of both projects and of Nerd Fonts ship beside adroit's.
- The icons look the same whatever the console's font, and may differ in
  shape from those the user's terminal draws.
