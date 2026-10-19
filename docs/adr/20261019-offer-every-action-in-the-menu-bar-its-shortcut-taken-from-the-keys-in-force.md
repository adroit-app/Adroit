# Offer every action in the menu bar, its shortcut taken from the keys in force

Status: Accepted

## Context

The menu bar held only the entries macOS and the window toolkit put there.
adroit's own actions, such as choosing where a session works, could be found
only by knowing their keys or reading the documentation.

## Decision

Every action that is not only a key has an entry in the menu bar. An entry
shows as its shortcut a Command key bound to its action in the bindings in
force, one that runs it wherever the keys are. The menu runs that key itself,
and the window leaves the key to it. An action with no such key shows none.

## Alternatives considered

- Show adroit's default keys in the menu. They are wrong once the user
  rebinds a key.
- List the entries without shortcuts. The menu would not teach the keys,
  which is half of what it is for.
- Show every binding, sequences and keys without Command included. A menu
  runs its shortcut itself, so a key without Command would be taken from
  typing, and a sequence cannot be a menu's shortcut.

## Consequences

- A new action needs an entry, unless it is only a key.
- A Command key bound in one area only, or meaning something else in some
  area, is no shortcut and stays the window's.
- The window has to know which Command keys the menu holds, and learn them
  again when the bindings change.
- An entry's key runs it while the keys are in a text field or another of
  adroit's windows too.
