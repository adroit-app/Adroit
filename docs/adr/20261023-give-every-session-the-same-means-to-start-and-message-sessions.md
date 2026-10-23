# Give every session the same means to start and message sessions

Status: Accepted

## Context

Delegation first needed a session chosen at launch as a Leader, which then
pushed every task through requests, attempts and verification, so small work
paid for machinery it did not need, and a session that was not a Leader could
not hand work on when it grew. The agent is better placed than a launch choice
to judge when to delegate. Claude Code's own messaging between sessions leaves
Codex out and addresses sessions by names that change or repeat.

## Decision

There is one kind of session. Every session adroit starts can start sessions,
message any other, and report to the session that started it. adroit carries
the messages, addressed by session identity, and hands each to the receiving
agent through that agent's own way of hearing from outside; handing it over is
receipt. Which session started which, and the messages, are all the work host
keeps between sessions: no request, task or verdict.

## Alternatives considered

- Choose Standalone or Leader at launch, or turn a session into a Leader on
  first delegation. Two kinds of session remain, and so does the machinery.
- Leave delegation to the agent's subagents. They run out of the user's sight
  and reach, and other agents cannot take part.
- Have adroit verify results before they count. The user watches the sessions;
  the session that receives a result checks it.
- Let sessions message only their starter and those they started. Unrelated
  sessions sharing a checkout could only coordinate through the user.
- Ask for an acknowledgement of every message. Receipt becomes work, and costs
  a turn.

## Consequences

- Starting a session asks only where it runs.
- Every session adroit starts carries adroit's bridge; sessions it did not
  start can neither send nor receive.
- A message costs its recipient a turn, so sessions are told to send only what
  the recipient has to act on.
- Who talked to whom outlives the sessions and the window.
