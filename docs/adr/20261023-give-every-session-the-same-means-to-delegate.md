# Give every session the same means to delegate

Status: Accepted; supersedes
20261021-choose-session-orchestration-at-launch.md,
20261020-keep-request-ownership-independent-of-working-directories.md,
20261022-confine-lifecycle-authority-to-app-actions.md and
20261022-hold-only-the-run-whose-record-cannot-be-read.md.

## Context

A session was chosen at launch to be Standalone or a Leader, before the work
had shown how large it was. A Standalone session could not delegate when the
work grew. A Leader had to delegate everything through requests, tasks,
attempts and verified candidates, so a small change paid for all of it. The
agent itself is better placed to judge when to delegate, as it already judges
when to use its own subagents.

## Decision

There is one kind of session. Every session Adroit starts can start sessions
of its own, send messages to other sessions, and report to the session that
started it. Which session started which is the only structure Adroit keeps
between sessions: it records no request, task or verdict, and a session that
starts others runs under no different policy.

## Alternatives considered

- Let the agent choose Standalone or Leader at launch. The choice still comes
  before the work shows its size, and a Leader still pays for the machinery.
- Turn a session into a Leader when it first delegates. Two kinds of session
  remain, and so does the machinery that made delegation heavy.
- Leave delegation to Claude Code's subagents and teams. They run inside one
  Claude Code, out of the user's sight and reach, and Codex cannot take part.
- Keep Adroit verifying results before work counts as done. The user watches
  the sessions and can step into any of them; the session that receives a
  result checks it by running checks or starting a reviewer.

## Consequences

- Starting a session asks only where, as it did before roles existed.
- Every session Adroit starts carries Adroit's control bridge.
- A result is as trustworthy as the session that receives it makes sure it is;
  Adroit records no judgement of it.
- Requests, tasks, attempts and candidates, and the state that held them, are
  removed; what an existing store holds of them is not carried over.
