# Let every session open sessions beside it that answer to the user

Status: Accepted

## Context

A session could start sessions to do parts of its work, each told only its
first prompt, each doing one piece and reporting back before it ended. Every
handoff and every report lost detail, and the starting session passed a
summary of the reports on to the user. Follow-up work went to a new session
that rebuilt what the ended one had known. The plan stayed with the starter
while the work was done elsewhere, so a change of plan arrived only with the
next report. Within one conversation, an agent runs what can go alongside
through its own subagents and tools, and gets their results back in the same
context.

The same loss comes back the other way. A line of work that the user has to
steer — a design to choose, a draft to review, an interface to approve —
given to a subagent reaches the user only through the session that started
it, and the user cannot step into it. Such lines often turn up inside a
session's own work, and their briefs carry what they need all the same.

## Decision

There is one kind of session, and every session Adroit starts can open
sessions beside it and message any other. A session it opens is like one the
user started: it answers to the user, reports to no one, ends only when the
user or the agent ends it, and can open sessions too.

Who will steer the work decides where it runs. A session opens one for a line
of work the user will want to steer themselves: one that needs their decisions
or review along the way, lands as a change of its own, or lives in another
repository, wherever it turned up, so long as what it needs fits in a first
prompt. What comes back to the session to fold into its own answer runs in its
agent's subagents and tools, and what follows from what it already knows stays
with it; a session never opens one to carry out a step of its own work, or
only to have something run alongside.

Sessions talk as peers: a message tells what the recipient should know or
asks what it knows, and never assigns it work; the recipient weighs it as any
other input and still answers to the user, and the sender plans nothing on a
reply coming. Adroit carries the messages, addressed by session identity, and
hands each to the receiving agent through that agent's own way of hearing from
outside; handing it over is receipt. Which session opened which, and the
messages, are all the work host keeps between sessions.

## Alternatives considered

- Let a session start workers that report back and end. The work reaches the
  user through a summary of summaries, and follow-up work starts from nothing.
- Keep a worker after its report for follow-up work from its starter. The
  starter still stands between the user and the work, and keeps the plan away
  from the session carrying it out.
- Open a session only for work that needs little of what the opener knows.
  Lines the user has to steer mostly come out of the opener's own work, so
  they go to subagents, and the user's decisions pass through the opener.
- Open a session for whatever can run alongside. Steps of one answer scatter
  across consoles, and the user is left to gather them.
- Leave every new session to the user. A separate line of work found inside a
  session would wait for the user to start it and write what it needs.
- Let sessions message only those they opened or were opened by. Unrelated
  sessions sharing a checkout could only coordinate through the user.
- Ask for an acknowledgement of every message. Receipt becomes work, and costs
  a turn.
- Let a message assign work and wait for its result. Workers come back through
  the messages, with the same summary between the user and the work.
- Keep sessions from talking at all. A session would rebuild from a diff what
  the session that made it could say.

## Consequences

- Work whose result returns to a session runs in its own agent's subagents
  and tools; Adroit takes no part in it.
- Work asked for in parallel splits two ways: lines the user will steer go to
  sessions, and the rest to subagents.
- A session opening another writes it a first prompt that carries all it
  needs, since the new session starts from nothing else.
- No session waits on another's result, so nothing has to bound how deep
  sessions open sessions.
- Every session Adroit starts carries Adroit's bridge; sessions it did not
  start can neither send nor receive.
- A message costs its recipient a turn, so sessions are told to send only what
  the recipient has to act on or answer.
- Who opened whom and who told whom what outlive the sessions and the window,
  and the user can read every exchange.
