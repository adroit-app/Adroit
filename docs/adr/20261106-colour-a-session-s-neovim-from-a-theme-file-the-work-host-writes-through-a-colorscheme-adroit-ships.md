# Colour a session's Neovim from a theme file the work host writes, through a colorscheme Adroit ships

Status: Accepted

## Context

A Neovim the user runs in a console, as a Neovim terminal or from a shell,
colours itself with the colorscheme its configuration names on the work
machine. That colorscheme need not match the theme the console is drawn in,
and the work machine need not have one that does. The work host already
chooses the theme in force and holds its colours, wherever it runs, as
20261024-have-the-work-host-read-the-system-s-appearance-and-choose-the-theme.md
decides; Neovim has no way to learn them but through what its session is
given.

## Decision

The work host writes the colours of the theme in force to a file of its
environment, rewrites it whenever it rethemes the consoles, and names it in
the environment of every session and terminal it starts. Adroit ships one
fixed Neovim colorscheme, put on the runtime path of every Neovim a session
runs through the data folders its environment names. The colorscheme takes
Neovim's own default highlights, recolours them from the file, and follows the
file while it is in use. The user opts in by choosing the colorscheme; Adroit
never chooses it for them.

## Alternatives considered

- Leave Neovim to a colorscheme of the theme's name installed on the work
  machine. Nothing guarantees one is, nor that it matches the theme Adroit
  draws.
- Have Neovim draw with the console's sixteen indexed colours, with
  `notermguicolors`. It matches the theme, but gives up every shade between
  them that true colour allows.
- Have Neovim query the terminal's palette. Adroit already holds the colours,
  and a change between two themes of the same kind sends Neovim no
  notification to query again.
- Write a colorscheme for each theme. It mixes code with data, and every
  change to how a theme is turned into highlights has to be written again into
  every file.
- Tell each Neovim its colours over RPC. The Neovim command is the user's to
  configure, and a Neovim started from a shell is not one Adroit can reach.
- Name a folder for the user to add to the runtime path themselves. Opting in
  then takes changing the runtime path as well as choosing the colorscheme,
  where the data folders leave only the choice.

## Consequences

- A theme change, by setting or by the system's appearance, reaches a Neovim
  that chose the colorscheme without restarting it.
- The colorscheme follows Neovim's default highlights, so it changes with the
  Neovim it runs in.
- A session's data folders start with one of Adroit's, which every program in
  the session that reads them also sees.
