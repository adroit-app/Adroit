# Keep the prompt's text for each tab

Status: Accepted

## Context

The window has one prompt, which sends to whatever is shown. With one text for
every tab, a message written for one session stayed in the prompt when another
was shown, and was sent there.

## Decision

What is written in the prompt belongs to the tab shown while it is written.
Showing another tab sets it aside and brings back that tab's.

## Alternatives considered

- One text for every tab, so a draft can be carried to another session.
  Sending a message to a session it was not written for is easy and cannot be
  taken back.
- A Neovim buffer per tab. Everything that reads or fills the prompt would have
  to name a buffer, and a restarted Neovim would lose every tab's text at once.

## Consequences

- Every change to the prompt's text goes through one place that knows which
  tab it is for.
- A tab's undo history and cursor are not kept across switches.
