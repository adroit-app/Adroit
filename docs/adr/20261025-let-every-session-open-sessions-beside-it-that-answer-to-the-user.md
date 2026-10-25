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

## Decision

There is one kind of session, and every session adroit starts can open
sessions beside it and message any other. A session it opens is like one the
user started: it answers to the user, reports to no one, ends only when the
user or the agent ends it, and can open sessions too. A session opens one for a
separate line of work that needs little of what it knows, not to carry out a
part of its own. Sessions talk as peers: a message tells what the recipient
should know or asks what it knows, and never assigns it work; the recipient
weighs it as any other input and still answers to the user, and the sender
plans nothing on a reply coming. adroit carries the messages, addressed by session identity,
and hands each to the receiving agent through that agent's own way of hearing
from outside; handing it over is receipt. Which session opened which, and the
messages, are all the work host keeps between sessions.

## Alternatives considered

- Let a session start workers that report back and end. The work reaches the
  user through a summary of summaries, and follow-up work starts from nothing.
- Keep a worker after its report for follow-up work from its starter. The
  starter still stands between the user and the work, and keeps the plan away
  from the session carrying it out.
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

- Work a session divides runs in its own agent's subagents and tools; adroit
  takes no part in it.
- No session waits on another's result, so nothing has to bound how deep
  sessions open sessions.
- Every session adroit starts carries adroit's bridge; sessions it did not
  start can neither send nor receive.
- A message costs its recipient a turn, so sessions are told to send only what
  the recipient has to act on or answer.
- Who opened whom and who told whom what outlive the sessions and the window,
  and the user can read every exchange.
