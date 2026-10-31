# Run each agent through an adapter that asks the agent what it offers

Status: Accepted

## Context

Adroit was built around Claude Code, and Codex followed. The agents differ in
how they start, take settings and images, report state and expose what they
offer: commands, skills, models and usage limits. What they offer differs by
account and release and changes without notice. Agents offer no API for what
a conversation did, the links it brought up, the subagents it ran, the prompts
sent to it, but they write each conversation's transcript and their prompt
history as they go.

## Decision

Each agent runs through an adapter that owns everything particular to it: its
launch and settings, its integrations, how it reads its screen and its
records. What an agent offers is asked of the agent itself, through a
short-lived process that runs no turn, and only what the agent cannot be asked
for is kept by the adapter. What a conversation did is read from the records
the agent writes, as they grow; Adroit keeps no record of its own of it. A
model the agent does not list is refused when it is chosen, before a session
starts; while no list is known, models are not checked.

## Alternatives considered

- Make the agent's executable a configurable command. One agent's arguments and
  requests would still reach another.
- Branch on the agent wherever Adroit needs it. Every new agent would touch
  the whole application.
- Keep tables of commands and models in Adroit. They drift with every release
  and know nothing of the account's own.
- Read the agent's configuration folders directly. They miss what is built in
  or comes from plugins and servers.
- Ask through an SDK or ACP adapter. It adds a runtime beside the agent, and
  answers for its own session rather than the terminal one Adroit runs.
- Read what a conversation did off the console. What scrolled off is gone,
  wrapped text has to be pieced together, and a prompt or tool input never
  appears whole.
- Install hooks that report to Adroit. It writes into the user's agent
  settings, and sessions started before Adroit tell nothing.
- Pass any model and report the failure. The mistake shows only after a
  session has started.

## Consequences

- Adding an agent is adding an adapter; agents share the terminal and prompt
  without promising the same integrations.
- The requests Adroit makes and the records it reads are internal to each
  agent and unversioned; a release that changes one leaves that feature empty,
  without an error, until Adroit follows.
- A model an agent accepts but does not list cannot be chosen per session.
- Shells have no conversation, and so nothing of this.
