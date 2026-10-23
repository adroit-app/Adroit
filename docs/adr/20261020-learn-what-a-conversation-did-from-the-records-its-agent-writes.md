# Learn what a conversation did from the records its agent writes

Status: Accepted

## Context

What a session brings up, the subagents it runs and the prompts sent to it
scroll out of the console or never appear there whole. Agents offer no API to
ask for them, but they write each conversation's transcript, and their prompt
history, as they go, for sessions started outside adroit too.

## Decision

The links a session brought up, its subagents and how each stands, and the
prompts to recall are read from the records the agent writes, as they grow.
adroit keeps no record of its own of them.

## Alternatives considered

- Read the console. What scrolled off is gone, wrapped text has to be pieced
  together, and a prompt or tool input never appears whole.
- Install hooks that report to adroit. It writes into the user's agent
  settings, and sessions started before or outside adroit tell nothing.
- Keep a history of what adroit sends. It misses everything sent elsewhere.

## Consequences

- The agents' record formats, which they do not promise, are interfaces adroit
  depends on; a change can empty a list without an error.
- Shells have no conversation, and so none of these.
