# Keep the prompt's text for each tab

Status: Accepted

## Context

The window has one prompt, and Ctrl+Enter sends it to whatever tab is shown.
With one text for every tab, a message written for one session stayed in the
prompt when another tab was shown, and was sent there.

## Decision

What is written in the prompt belongs to the tab shown while it is written.
Showing another tab sets it aside and brings back that tab's.

## Alternatives considered

- Keep one text for every tab, so a half-written message can be carried to
  another session. Sending to a session a message was not written for is easy
  to do and cannot be taken back.
- Give each tab a Neovim buffer of its own, keeping its undo history and
  cursor. Everything that reads or fills the prompt would have to name a
  buffer, and a restarted Neovim would lose every tab's text at once.

## Consequences

- Every change to the prompt's text, from typing aside, goes through one
  place that knows which tab it is for.
- A tab's undo history and cursor are not kept across switches.
- Text for a tab that closes is not lost: it stays for the tab shown next.
