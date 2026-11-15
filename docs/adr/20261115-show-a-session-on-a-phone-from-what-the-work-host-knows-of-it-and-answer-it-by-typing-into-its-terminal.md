# Show a session on a phone from what the work host knows of it, and answer it by typing into its terminal

Status: Accepted

## Context

A paired client on a phone reaches the same work host and terminal host as the
machine's own windows. An agent's terminal interface is drawn for a desktop's
width, and the terminal host gives a session the size of whichever client
typed into it last, so a phone and a desktop drawing one console would reflow
it for each other. The work host knows how each session stands, what it asks
the user and what it does, from the programs' status reports and the hooks
their agents run.

## Decision

A phone shows a session only from what the work host knows of it, and never
draws its terminal. What the user answers there, the work host types into the
session's terminal as it types a prompt, or passes on through a way the agent
offers for answering from outside that leaves its own dialog shown. A question
the work host cannot answer so is shown as waiting on the user, to be answered
at a window.

## Alternatives considered

- Draw the terminal on the phone. It is unreadable at a phone's width, and
  typing from the phone would reflow the session under the desktop's console.
- Answer through a hook that waits for the user. The agent's own dialog would
  be hidden from every window while it waits.
- Drive the phone's sessions through the agent's SDK or headless mode. Those
  are sessions of their own, apart from the terminal the windows show.
- Leave phones to the agent vendor's own remote apps. Each serves its own
  agent alone, and none shows the sessions as Adroit holds them.

## Consequences

- A phone needs no terminal emulator and speaks only the work protocol.
- How each answer is typed is the adapter's for each agent, and breaks with
  the agent's screen like the rules that read it; until Adroit follows, that
  answer is left to a window.
- A session is shown on a phone only as fully as the work host knows it; an
  agent without hooks shows how it stands and takes prompts, and little more.
- A window and a phone can answer the same question, and the first answer
  reaches the agent.
