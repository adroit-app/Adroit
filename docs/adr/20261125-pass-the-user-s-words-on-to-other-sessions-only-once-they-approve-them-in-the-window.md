# Pass the user's words on to other sessions only once they approve them in the window

Status: Accepted

## Context

What the user decides in one session often holds for several, and a message or
a request from that session reaches the others as an agent's: it approves
nothing and answers nothing. The user had to say it again in each. A session
that could speak for the user whenever it chose could approve anything
anywhere, as could one misled by what it read.

## Decision

A session can propose to pass the user's words on to other sessions, and
nothing more. The words go nowhere until the user, shown them whole with whom
they reach, approves them in Adroit's own window; each proposal is approved by
itself, as it stands. Once approved, the work host hands them over as the
user's own words, marked with the session they came through and when they were
approved, and they count as an instruction the user typed there. A session they
reached does not pass them on: what it proposes is refused until the user writes
to it.

## Alternatives considered

- Let a session speak for the user when the user tells it to. Only its agent
  can judge whether the user did, and an agent misled judges wrong.
- Let the user grant a session the right once. A session misled later would
  still hold it.
- Approve through the agent's own permission prompt for the tool. A permission
  mode that asks nothing would approve it.
- Let a program with a key the user gave approve. Nothing shows that the user
  was asked.
- Type the words into each session's terminal as a prompt. They would mix with
  what the user is writing there, and could answer a dialog.

## Consequences

- Passing words on costs the user a look and an answer each time.
- The approval keeps out an agent that calls its tools or runs a command, not a
  program outside a sandbox that runs as the user and sets out to speak to the
  work host as a window does.
- An agent is told the words are the user's by the instructions Adroit gives it,
  so one that was started before takes them as a message.
- A session whose agent asks its user in a way of its own, such as a dialog, is
  still answered there.
