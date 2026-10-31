# Let the user's configuration change what Adroit's Neovims only suggest, and hold what the window relies on

Status: Accepted

## Context

The prompt and the console's reader are Neovims that read a configuration of
the user's. Adroit set everything it wanted once startup had finished, after
that configuration, so a user could not change even a look Adroit had only
chosen as a default, such as line numbers or completion options. Some of what
Adroit sets, though, is what the window is built on: the rows it sizes to the
prompt's text, the scratch buffer it reads and fills, the columns it lays the
console's images over. A configuration or a plugin that changed those would
leave the window broken rather than styled differently.

## Decision

What Adroit sets in its Neovims is of two kinds. What it only suggests is set
before the user's configuration, which may change any of it. What the window
relies on is set after the configuration and set again whenever anything
changes it later.

## Alternatives considered

- Setting everything after the configuration, as before. Nothing Adroit
  suggests could be changed.
- Setting everything before the configuration and trusting it. A setting
  copied from an everyday Neovim, a status line or line numbers, would break
  the window with no sign of why.
- Setting what the window relies on only once, after the configuration. A
  filetype plugin or a plugin loaded later would still change it.

## Consequences

- Each setting Adroit makes must be placed in one kind or the other, and
  moving one between them changes what users can configure.
- What the window relies on cannot be changed by any configuration, even by a
  user who would accept what breaks.
