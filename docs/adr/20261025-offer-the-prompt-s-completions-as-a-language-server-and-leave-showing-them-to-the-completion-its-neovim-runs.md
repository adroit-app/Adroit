# Offer the prompt's completions as a language server, and leave showing them to the completion its Neovim runs

Status: Accepted

## Context

The prompt offers sessions, paths and commands after `@`, `/` and `$`. adroit
found them and opened Neovim's completion menu with them itself, on every
change, and held the completion options it needed. A configuration that
completes with a plugin of its own then had two completions on the same
keys, fighting over the menu and over options adroit would not let it change.

## Decision

What the prompt offers is served by a language server running inside the
prompt's Neovim. adroit only suggests showing it with Neovim's own
completion; a configuration may turn that off and show the offers with any
completion that reads a language server.

## Alternatives considered

- Keep opening the menu itself, and let a configuration turn it off. A plugin
  would still have no way to read what adroit offers.
- Offer them through `'omnifunc'` or `'completefunc'`. Completion plugins need
  an adapter of their own for these, and nothing tells them when to ask.
- A language server run as a program of its own. It would need starting,
  stopping and a channel to the window's data, for what the prompt's Neovim
  already holds.

## Consequences

- Completion plugins find the prompt's offers with no setting of adroit's.
- What is offered is decided apart from how it is shown, and a test can ask
  the language server without a menu.
- The prompt's Neovim lists a language server named adroit, and a
  configuration's handlers for attaching ones run for it too.
- Completion options are no longer held: a configuration can make Neovim's
  own completion behave otherwise.
