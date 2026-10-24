# Open in a tab of its own, and look in one preview tab

Status: Accepted

## Context

Opening a row of the tree put what it stood for in the focused window, in
place of what that window showed, as a Neovim file explorer does. Neovim
offers `Ctrl-o` to go back to what a window showed; adroit keeps no such
history, so what was replaced could only be found again in the tree. People
coming from other applications expect what they open to come in a tab of its
own, and stepping through sessions one by one still wants a single place that
follows along.

## Decision

Nothing opened replaces what a window shows. Opening something goes to a
window already showing it, or else to a new tab. Looking at something without
opening it shows it in a single preview tab, drawn apart from the others,
whose content the next thing looked at replaces; the preview becomes an
ordinary tab once what it shows is opened or worked in.

## Alternatives considered

- Keep replacing what the focused window shows, and add a history to go back
  through. Every window would carry a history the user has to keep in mind,
  for a gesture the tabs already give.
- Open everything in a new tab, with no preview. Stepping through sessions or
  glancing at rows would leave a tab behind for each.

## Consequences

- Tabs accumulate as things are opened, and are closed by hand, with a mouse
  as well as with keys.
- Every way of showing something says whether it opens or only looks.
