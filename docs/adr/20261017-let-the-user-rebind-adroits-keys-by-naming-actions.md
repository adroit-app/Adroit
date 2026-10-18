# Let the user rebind adroit's keys by naming actions

Status: Accepted

## Context

adroit is driven from the keyboard, and the people it is for bring keys of
their own from Vim and from other tools. Every key was fixed in the code, so a
key that clashed with a habit, or with a key Claude Code wants, could not be
moved.

## Decision

Every key adroit handles itself runs a named action. Key sequences are bound to
actions per area of the window, adroit ships its own bindings as defaults, and a
file of the user's own overrides, adds to or removes them.

## Alternatives considered

- Offer a few preset layouts to choose from. Habits differ in the details a
  preset fixes, and every new action would need a place in each preset.
- Let the user remap keys before adroit sees them. A key would change in every
  area at once, though the same key means different things in the sidebar and
  in the console.
- Configure the keys through the prompt's Neovim. Most of the window is not
  Neovim, and its mappings could not reach the sidebar or the console.

## Consequences

- Each action has a name that stays stable, since the user's file refers to it.
- What the window says about its keys has to come from the bindings in force,
  not from the defaults.
- The prompt's own keys stay Neovim's, configured in Neovim.
