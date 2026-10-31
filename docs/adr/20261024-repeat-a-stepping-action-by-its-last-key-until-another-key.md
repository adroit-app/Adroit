# Repeat a stepping action by its last key until another key

Status: Accepted

## Context

The window's commands follow a prefix, so stepping through tabs, waiting
sessions or workspaces, or resizing a window a few steps, takes the whole
sequence each time. The step is often taken several times in a row.

## Decision

An action that steps through something repeats: once a sequence bound to it
runs, the key that ended it, or another that ends a repeating binding after
the same keys, runs again alone. Any other key ends the repeating and does
what it does. Which actions repeat is the action's to say, not the binding's,
so the user's own sequences repeat as Adroit's do.

## Alternatives considered

- End the repeating after a pause, as tmux does. A pause of the user's own,
  to look at the tab reached, would end it, and the time is one more setting.
- Repeat every sequence. A split, a close or a move to another window would
  take the key meant for what comes next.
- Mark repeating in the file of bindings, binding by binding. Every user
  would mark the same actions over again.

## Consequences

- A key typed right after a repeating step that is also that step's last key
  steps again, rather than reaching where the keys are.
- The window shows that its keys repeat, as it shows its other modes.
