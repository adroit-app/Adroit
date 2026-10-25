# Ship a pinned Neovim with the app for the Neovims it embeds, and open files in the user's

Status: Proposed

## Context

The prompt and the console's reader are Neovims the window attaches to as
their interface. The window is built on what that interface reports and on
the options adroit holds in them, and both have changed between Neovim
releases. They ran whichever Neovim the machine had, so adroit worked only
after Neovim was installed beside it, with whatever version a package manager
had put there and later upgraded under it.

Neovim plays a second part in adroit: a file or a directory opened from the
window opens in Neovim in a terminal. That is the user's everyday editor, with
the configuration and plugins they keep for it.

## Decision

The app carries a Neovim of a version adroit chooses and tests against, and
the Neovims adroit embeds run it. A user may name another Neovim for them
instead. Files and directories opened from the window still open in the
Neovim the user has installed.

## Alternatives considered

- Require Neovim to be installed, as before. The version the window meets
  would be the machine's choice, and an upgrade outside adroit could break the
  prompt with no change to adroit.
- Prefer an installed Neovim and fall back to the one carried. Which version
  the window meets would still differ by machine, and so would what breaks.
- Open files in the carried Neovim too. The user's everyday configuration is
  written for the Neovim they installed, and opening a file is meant to bring
  up their own editor.

## Consequences

- Installing the app is enough for the prompt and the reader.
- The embedded Neovims change version only with adroit's releases, and
  Neovim's fixes reach them only through those releases.
- The app grows by a Neovim and its runtime.
- A configuration written for the embedded Neovims must work with the version
  carried, unless the user names another Neovim and accepts what the window
  was not tested against.
- A work host run apart from the app, on another machine or from a
  development build, still needs a Neovim found where it runs.
