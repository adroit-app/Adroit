# Open everything in a tab of its own, with no preview

Status: Accepted

## Context

Opening a row of the tree put what it stood for in place of what the focused
window showed. Adroit keeps no history of what a window showed, so what was
replaced could only be found again in the tree. A single preview tab, which
the next thing looked at replaced, then took what was only looked at. Every
way of showing something had to choose between opening and looking. The user
had to tell the preview apart from the other tabs and learn when it turned
into an ordinary tab.

## Decision

Nothing shown replaces what a window shows. However it is asked for, showing
something goes to a window already showing it, or else to a new tab.

## Alternatives considered

- Keep one preview tab for what is only looked at. Stepping through sessions
  would leave no tab behind, but every way of showing something would still
  have to say whether it opens or looks. A tab that changes under the user
  would still have to be told apart and kept by hand.
- Show what is opened in the focused window, with a history to go back
  through. Every window would carry a history the user has to keep in mind,
  for a gesture the tabs already give.

## Consequences

- Tabs accumulate as things are shown, stepping through sessions included,
  and are closed by hand.
- Every tab is numbered, and moves like any other.
