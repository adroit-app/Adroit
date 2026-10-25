# Run adroit's actions from the prompt through mappings of its Neovim

Status: Accepted

## Context

Some of adroit's actions are run from the prompt: sending it, recalling the
prompts sent, taking the prompt an agent suggests, passing a key to the
agent. The window took their keys before Neovim did, and gave a key back to
Neovim when Neovim seemed to need it, judged by whether it showed its own
completion menu. A completion plugin draws a menu the window cannot see, and
a configuration mapping one of those keys never received it.

## Decision

The window gives the prompt's Neovim every key but its own window-wide ones.
adroit's actions are run from the prompt by a command of its Neovim, and the
keys for them are mappings adroit suggests, which a configuration may change
like any other.

## Alternatives considered

- Keep taking the keys in the window and judge better when Neovim needs them.
  The window cannot see a plugin's menu or a configuration's mappings, so the
  judgement stays a guess.
- Let the keys file rebind them. A key would still be decided outside
  Neovim, against what the configuration maps it to.

## Consequences

- Neovim's own rules decide which mapping a key runs, so a completion
  plugin's mapping wins wherever the configuration puts it first.
- The prompt's keys for adroit's actions are changed in the prompt's
  configuration, not in the keys file, and the keys help no longer lists
  them.
- Any action can be run from the prompt, by a key the configuration maps.
