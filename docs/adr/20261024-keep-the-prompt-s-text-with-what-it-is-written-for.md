# Keep the prompt's text with what it is written for

Status: Accepted

## Context

The window has one prompt, which sends to whatever the focused window shows.
With one text for everything, a message written for one session stayed in the
prompt when another was shown, and was sent there. The same session can be
shown in several tabs and windows, and in several windows of adroit at once.

## Decision

What is written in the prompt belongs to the session, or the session not
started yet, that the focused window showed while it was written. Showing
another sets it aside and brings back what was written for that one. The work
host keeps what is written, so it outlives the window.

## Alternatives considered

- One text for everything, so a draft can be carried to another session.
  Sending a message to a session it was not written for is easy and cannot be
  taken back.
- A text per tab. The same session shown in two tabs would have two texts,
  and closing a tab would lose what was written for a session still running.
- A Neovim buffer per session. Everything that reads or fills the prompt would
  have to name a buffer, and a restarted Neovim would lose every text at once.

## Consequences

- Every change to the prompt's text goes through one place that knows what it
  is for.
- Undo history is not kept across switches. The cursor is, by the window
  alone, so it does not outlive the window.
- Two windows writing for the same session do not see each other's text; the
  one kept last is what comes back.
