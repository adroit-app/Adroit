# End a started session once it reports

Status: Accepted; supersedes 20261021-end-worker-sessions-after-their-final-report.md.

## Context

Sessions started to do one piece of work sit idle once it is done, each
holding an agent process, until someone ends them. Their reports and the files
they worked on are what the starting session needs afterwards.

## Decision

A session started by another ends once it has reported to the session that
started it and its turn is over. Its report and its working directory remain.
Ending a session does not end the sessions it started.

## Alternatives considered

- Keep a reporting session for follow-up questions. Idle sessions accumulate
  without bound; follow-up work can go to a new session that starts from what
  the first one left.
- End the sessions a session started when it ends. Their work is still in
  progress and still visible to the user, who may want it finished.

## Consequences

- A session finishes and checks its work before it reports.
- Reports to a session that has ended stay readable.
- A session whose starter has ended runs on until it reports or the user ends
  it.
