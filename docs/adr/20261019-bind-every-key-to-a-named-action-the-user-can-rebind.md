# Bind every key to a named action the user can rebind

Status: Accepted

## Context

adroit is driven from the keyboard, and the people it is for bring keys of
their own from Vim and other tools. Keys fixed in the code could not be moved
when they clashed with a habit or with a key a program in a session wants.

## Decision

Every key adroit handles itself runs a named action. Keys are bound to actions
per area of the window; adroit's own bindings are defaults that a file of the
user's overrides, adds to or removes. Everything the window says about its
keys, the menu bar included, is taken from the bindings in force.

## Alternatives considered

- Preset layouts to choose from. Habits differ in the details a preset fixes,
  and every new action needs a place in each preset.
- Remap keys before adroit sees them. A key would change in every area at
  once, though the same key means different things in different areas.
- Configure the keys through the prompt's Neovim. Most of the window is not
  Neovim.
- Show the default keys in menus and hints. They are wrong once the user
  rebinds one.

## Consequences

- An action's name is an interface: the user's file refers to it.
- The prompt's own keys stay Neovim's, configured in Neovim.
