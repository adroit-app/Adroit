# Close the tab with Cmd+w and the window with Cmd+Shift+w

Status: Accepted

## Context

adroit is one window that holds its sessions and shells as tabs. Cmd+w was
left to macOS's own Close entry, which closed that window and so quit
adroit, while closing a tab had no Command key at all. Apps built around tabs
in one window, browsers and terminals, close the tab with Cmd+w and the
window with Cmd+Shift+w.

## Decision

Cmd+w closes the current tab and Cmd+Shift+w closes the window. Both are
adroit's own actions, bound in its keys like any other, and Command with `w`
is no longer kept for the menu. Closing from the Settings window closes the
Settings window.

## Alternatives considered

- Keep Cmd+w on macOS's Close. Closing the one window quits adroit, too heavy
  a result for a key pressed as often as closing a tab, and one close to Vim's
  `Ctrl+w`.
- Let Cmd+w close the window when no tab is left, as a browser does. The
  window is not one of many: closing it quits adroit, and a run of Cmd+w to
  clear the tabs would quit it as well.

## Consequences

- macOS's Close entry is gone from the menu bar; an entry of adroit's runs
  closing in its place, and shows whatever key the bindings give it.
- Unbinding Cmd+w leaves the Settings window to be closed by its title bar.
