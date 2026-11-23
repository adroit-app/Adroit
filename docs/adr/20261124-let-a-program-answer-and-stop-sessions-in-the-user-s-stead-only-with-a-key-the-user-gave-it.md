# Let a program answer and stop sessions in the user's stead only with a key the user gave it

Status: Accepted

## Context

The user is not always where Adroit runs, and an assistant of theirs can ask
them elsewhere what a session asks. Every session can run commands as the user,
so a command that answers for the user would let an agent approve what its
permissions are there to stop.

## Decision

A program answers, interrupts and ends sessions only with a key the user gave it
in Adroit's own window, and only while the user lets programs do so, which is off
until they do. The key reaches Adroit where no other program reads it, is given
to no session, and is kept only as what tells it from another. An answer names
the question it answers and goes to the agent through the agent's own way of
being answered, which leaves its dialog to the user; it holds for that once. No
answer is typed.

## Alternatives considered

- Leave answering to the user in Adroit. A session waits for as long as the user
  is elsewhere.
- Let any program that runs as the user answer. An agent could approve its own
  requests.
- Tell sessions from other programs by their environment. An agent drops what it
  is told by.
- Give keys by a command. An agent could give itself one.
- Type answers into the terminal. A key can reach another dialog than the one
  the user was shown.

## Consequences

- A key keeps out an agent that runs a command, not a program outside a sandbox
  that sets out to get past it.
- Only what an agent takes an answer to through such a way can be answered; the
  rest waits for the user in Adroit.
- A turn is interrupted by the agent's own key for it, typed only where nothing
  waits on the user, as no agent offers another way.
