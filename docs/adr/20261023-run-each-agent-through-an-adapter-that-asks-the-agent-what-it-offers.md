# Run each agent through an adapter that asks the agent what it offers

Status: Accepted

## Context

adroit was built around Claude Code, and Codex followed. The agents differ in
how they start, take settings and images, report state and expose what they
offer: commands, skills, models and usage limits. What they offer differs by
account and release and changes without notice.

## Decision

Each agent runs through an adapter that owns everything particular to it: its
launch and settings, its integrations, how it reads its screen and its
records. What an agent offers is asked of the agent itself, through a
short-lived process that runs no turn, and only what the agent cannot be asked
for is kept by the adapter. A model the agent does not list is refused when it
is chosen, before a session starts; while no list is known, models are not
checked.

## Alternatives considered

- Make the agent's executable a configurable command. One agent's arguments and
  requests would still reach another.
- Branch on the agent wherever the window needs it. Every new agent would touch
  the whole application.
- Keep tables of commands and models in adroit. They drift with every release
  and know nothing of the account's own.
- Read the agent's configuration folders directly. They miss what is built in
  or comes from plugins and servers.
- Ask through an SDK or ACP adapter. It adds a runtime beside the agent, and
  answers for its own session rather than the terminal one adroit runs.
- Pass any model and report the failure. The mistake shows only after a
  session has started.

## Consequences

- Adding an agent is adding an adapter; agents share the terminal and prompt
  without promising the same integrations.
- The requests adroit makes are internal to each agent and unversioned; a
  release that changes one leaves that feature empty until adroit follows.
- A model an agent accepts but does not list cannot be chosen per session.
