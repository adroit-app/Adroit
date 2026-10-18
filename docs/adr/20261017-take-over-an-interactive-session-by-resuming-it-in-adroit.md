# Take over an interactive session by resuming it in adroit

Status: Accepted

## Context

Claude Code sessions are also started in other terminals, and Claude Code
lists them with the directory they run in. adroit cannot show them: another
terminal's pseudo-terminal cannot be read from outside it. Asking the user to
find that terminal defeats working from one window.

## Decision

adroit takes over a session running in another terminal by stopping its
process and resuming the conversation, under the same session id, in a session
of its own.

## Alternatives considered

- Show the session's screen in adroit. Another terminal's pseudo-terminal
  cannot be read from outside it.
- Type into the terminal the session runs in. Typing into another process's
  terminal is refused without root, and terminal multiplexers and emulators
  each need their own way, which adroit cannot tell apart.
- Resume a copy while the process keeps running. The conversation splits into
  two that drift apart.

## Consequences

- The terminal the session ran in loses it: Claude Code exits there.
- A turn in progress when the process stops is lost, so taking over has to be
  held back while the session works.
- Claude Code's own list of sessions stays a source for those running outside
  adroit.
