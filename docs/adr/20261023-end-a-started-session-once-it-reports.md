# End a started session once it reports

Status: Accepted

## Context

A session started to do one piece of work sits idle once it is done, holding
an agent process, until someone ends it. Its report and the files it worked on
are what its starter needs afterwards.

## Decision

A session started by another ends once it has reported to its starter and its
turn is over. Its report and working directory remain. Ending a session does
not end the sessions it started.

## Alternatives considered

- Keep a reporting session for follow-up questions. Idle sessions accumulate
  without bound; follow-up work can go to a new session.
- End the sessions a session started when it ends. Their work is in progress,
  and the user may want it finished.

## Consequences

- A session finishes and checks its work before it reports.
- A session whose starter has ended runs on until it reports or the user ends
  it.
