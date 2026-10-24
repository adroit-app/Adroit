# Keep a started session to its one piece of work

Status: Accepted

## Context

A session started by another is started for one piece of work. Once it is
done, the session sits idle, holding an agent process, until someone ends it;
what its starter needs afterwards is its report and the files it worked on.
The work can also turn out larger than it looked. Starting sessions of its own
would put work further from the user and from the session that answers for
it, and nothing would bound how many sessions follow.

## Decision

A session started by another session does the piece of work it was started
for and nothing more. It cannot start sessions: when its work outgrows it, it
tells its starter what it has found and how it would divide the rest, and the
starter decides. It ends once it has reported to its starter and its turn is
over, leaving its report and its working directory. Ending a session does not
end the sessions it started.

## Alternatives considered

- Let started sessions start sessions, to a fixed depth or freely. Work
  divided below the first level is decided where neither the user nor the
  starting session sees the whole of it, and sessions multiply without bound.
- Keep a reporting session for follow-up questions. Idle sessions accumulate;
  follow-up work can go to a new session.
- End the sessions a session started when it ends. Their work is in progress,
  and the user may want it finished.

## Consequences

- Every session is at most one step from a session the user started.
- A session finishes and checks its work before it reports.
- A session asked to divide the work can start new sessions from what the
  asking session has already done.
- A started session can still use its agent's own subagents.
- A session whose starter has ended runs on until it reports or the user ends
  it.
