# Take a session's links from its transcript, not its screen

Status: Accepted

## Context

The addresses, files and artifacts a session brings up scroll out of the
console, and a long address is wrapped across lines there. Getting back to
one meant scrolling back and copying it by hand. Claude Code offers no API
that tells what a conversation mentioned, but it writes every conversation
to a transcript as it goes.

## Decision

What a session brought up is read from the transcript Claude Code writes of
its conversation, as that transcript grows.

## Alternatives considered

- Read the console's screen and what scrolled off. What scrolled past the
  scrollback is lost, a wrapped address has to be pieced back together, and
  what tools read or wrote never appears whole on screen.
- Ask Claude Code. It has no way to be asked.

## Consequences

- The shape of Claude Code's transcript, which it does not promise, becomes
  an interface adroit depends on; a change to it can empty the list without
  an error.
- Only sessions with a conversation have links; shells have none.
- Which tool wrote or read a file is known, not only that it was named.
