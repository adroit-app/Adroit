# Keep the window's state in a core free of the UI, changed only by messages

Status: Accepted

## Context

The window's state lived in the UI's components, as signals that effects,
tasks and handlers read and wrote wherever they ran. A feature touched many
of them at once, the order in which they reacted to each other was implicit,
and none of it could be tested without running the UI. Adding to the window
had become slow and unsafe.

## Decision

The window's state is plain data held by a part of Adroit that knows nothing
of the UI and runs no program and touches no file. It changes only by
messages: each message is applied by one synchronous function that changes
the state and returns, as values, the effects it wants carried out. Rules that
keep one part of the state in step with another run after every message, in
an order the core fixes. The UI draws the state, turns what the user does
into messages, and carries out the effects, whose results come back as
messages.

## Alternatives considered

- Keep the state in the UI's components. Every rule stays tied to the UI's
  reactivity and untestable without it.
- Let the core carry out its effects itself. Testing a rule would run
  programs and wait on sockets.
- A state store of the UI framework's. The rules would still be written in,
  and tested with, the UI.

## Consequences

- The window's behaviour is tested by feeding messages and reading the state
  and the effects, without a UI.
- What changes often and in bulk, terminal screens and Neovim's grid, stays
  out of the state and is drawn from its source.
- The UI changes the state only from handlers and tasks, never while drawing.
- A new feature is a message, its handling and its effects in the core, and
  the markup that draws it.
