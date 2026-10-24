# Describe every action and setting once, in the code, and write their references from it

Status: Accepted

## Context

An action was named in one place, given its menu label and icon in others,
described in the specification, and bound by default in yet another; a
setting's type and default lived in the code and again in a table of the
specification. Every new action or setting touched them all, and the tables
fell behind the code without anything failing.

## Decision

Each action is described once, in one table in the code: its name, what the
menus call it, its icon and what it does; its default keys are bound in the
code's default bindings alone. Each setting
is described once, by its type and its documentation in the code. The
references listing every action and every setting are written from the code,
kept beside the specification, and a test fails when they differ from what
the code would write. The specification links to them instead of repeating
them.

## Alternatives considered

- Keep the tables by hand in the specification. They drift from the code.
- Write the references only when the documentation is built. They could not
  be read in the repository, and a change to them would not show in review.
- Make the specification the source and read it into the code. The code
  would depend on the layout of prose documents.

## Consequences

- Describing an action or a setting is a change to the code; the references
  are written again with it.
- Everything the window says of an action, the menu bar, the help and the
  Settings window, comes from the same row.
- The specification holds no table of actions or settings.
