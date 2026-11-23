# Let a program outside Adroit ask a session for work under its own name

Status: Accepted

## Context

A program of the user's own, such as an assistant, can start sessions from a
command, and then has to learn what came of them and have them go on. A
message between sessions hands over no work, since every session answers to
the user. A prompt is the user's own word, and what the user writes in a
console also answers the dialogs there.

## Decision

A program outside Adroit asks a session for work through a request: handed to
the agent as a message is, apart from its terminal, headed by the name the
program gives itself and by what it is. The agent takes it up as work on the
user's behalf, and not as the user's own word: a request approves nothing and
answers nothing that waits on the user. The program reads how a session stands
and what it answered from what the work host knows. A request gives it no way to
answer what a session asks the user.

## Alternatives considered

- Type the request into the session's terminal as a prompt. It reads as the
  user's own word, mixes with what the user is writing, and can answer a
  dialog that opened meanwhile.
- Send it as a message between sessions. A message hands over no work, so an
  agent is right not to act on it.
- Give the program the credential of a session or of the user. What it does
  could no longer be told from what they do.

## Consequences

- The name is the program's own word. Programs that run as the same user are
  kept from being mistaken for one another, not from impersonating.
- Whether a request is followed is the agent's to judge, as with any message.
- No request makes a session that waits on the user go on.
- A session asks another for nothing this way; it has requests of its own,
  where the user lets it.
