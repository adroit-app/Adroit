# Keep terminal resources alive independently of their tabs

Status: Accepted

## Context

Shell and Neovim programs can support a request while another session is
shown. Treating their tabs as process lifetimes makes changing the visible
workspace end useful tools, and a window-only query cannot serve workers
while the window is closed.

## Decision

Keep terminal resources alive independently of their tabs. A shell or editor
has an explicit terminal resource identity, separate from agent conversations
and from the views that show it. Closing a tab hides the resource; stopping
the resource ends its program.

## Alternatives considered

- End a shell or editor when its tab closes. A view operation would destroy
  a resource that background work may still need.
- Address tools by name and working directory through the window. Renaming,
  shared directories and a closed window would change what a worker can read.

## Consequences

- Terminal queries can use the daemon without a window.
- A terminal's optional request association is independent of its location.
- Shell and Neovim are terminal programs, not workers; embedded prompt and
  console Neovim instances remain window components.
