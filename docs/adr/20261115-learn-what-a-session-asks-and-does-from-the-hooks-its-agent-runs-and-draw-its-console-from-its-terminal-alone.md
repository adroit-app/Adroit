# Learn what a session asks and does from the hooks its agent runs, and draw its console from its terminal alone

Status: Accepted

## Context

Adroit knows that a dialog holds a session's keys only from its screen, and
never what the dialog asks. A screen changes with every release of its agent,
and a question, its choices or a tool's input never appear on it whole. The
transcript is written behind the turn, and says nothing of what waits on the
user. Agents run hooks at each step of a turn and hand them, structured, what
the agent is about to do, what it did and what it asks the user, and Adroit
already loads a plugin of its own into every Claude Code session it starts.

A client that cannot draw a terminal, such as a phone, can show a session only
from such knowledge.

## Decision

The work host learns what a session asks the user and what it does as it goes
from hooks that Adroit's own integration loads into the sessions it starts. A
hook hands what it is given to the work host and returns at once: it never
answers for the user and never holds the agent. A window still draws each
session's console from its terminal alone; what the hooks tell is shown beside
it, never in its place.

## Alternatives considered

- Draw the console from the hooks. Whatever an agent shows without a hook
  would be lost, and every change to a hook's input would change the console.
- Keep reading what a session asks off its screen. The question and its
  choices are never whole there, and the rules break with each release.
- Read it from the transcript alone. It is written behind the turn and holds
  nothing of what waits on the user.
- Let a hook wait for the user's answer. The agent's own dialog would stay
  hidden while it waits, so the agent would no longer behave as in a terminal,
  and a work host gone would stall the turn.

## Consequences

- A hook runs inside the agent's turn, so Adroit's hook has to return quickly,
  and failing to reach the work host costs the turn nothing.
- What the hooks tell is unversioned; a release that changes it leaves that
  knowledge empty until Adroit follows, and leaves the console as it was.
- A session started outside Adroit, or an agent without hooks, is known only
  from its screen, its status reports and its transcript, as before.
- The work host holds what the hooks told only while it runs; what a
  conversation did is still read from the transcript when it starts again.
