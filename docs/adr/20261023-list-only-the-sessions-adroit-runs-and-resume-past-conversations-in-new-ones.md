# List only the sessions Adroit runs, and resume past conversations in new ones

Status: Accepted

## Context

Agents run outside Adroit too: in other terminals, and in other instances of
Adroit. Adroit cannot show a session it does not run, and an outside session
looks the same to it whichever of these runs it, so another instance's
sessions showed up without what that instance knows of them, and could be
stopped from here. What the user wants from an outside session is mostly its
conversation, which every agent keeps after the session ends.

## Decision

Adroit lists only the sessions its own instance runs. A conversation an agent
kept, whichever agent and wherever it ran, is resumed in a new session of
Adroit's own, through the agent's own resume, without stopping or changing
anything that runs. A conversation a session of the instance already holds is
shown rather than resumed again.

## Alternatives considered

- Keep listing sessions running outside Adroit. They cannot be shown, other
  instances' sessions are listed without their ties, and stopping one from
  here reaches into another instance.
- Take an outside session over by stopping its process and resuming it here.
  The terminal it ran in loses it, a turn in progress is lost, and the same
  stop reaches other instances' sessions.
- Resume only Claude Code's conversations. Every agent keeps its
  conversations, and resuming one is particular to each agent, which its
  adapter already owns.

## Consequences

- Each agent's adapter says where its conversations are kept and how one is
  resumed.
- A conversation resumed while it still runs elsewhere continues in both
  places, which drift apart.
- Sessions running outside Adroit are not seen at all.
