# Let a session's agent name it once it knows the work

Status: Accepted

## Context

A session is named the moment its first prompt is sent, from that prompt's
first words. Those words are often a greeting, a question or a sentence cut
short, and in Japanese there are no spaces to say where a word ends, so the
names in the tree said little of the work. The agent reading the prompt knows
what the work is once it has answered, and knows when the work has turned to
something else.

## Decision

A session's agent names its session itself, through Adroit's own tools: once
it has answered the first prompt, and again when the work turns to something
else entirely. A name a person gave is never changed by the agent, and the
user can switch the agent's naming off. Until the agent names it, the session
keeps the name made from its prompt.

## Alternatives considered

- **Only better rules over the prompt's words.** No rule over the first words
  knows what the work is, and none notices when it changes.
- **Adroit asks a small model for a title.** It needs a login and a model of
  its own for every agent, costs a call per session and leaves Adroit waiting
  on it, while the session's own agent already knows the work.
- **Reading the title an agent keeps for its conversation.** Only some agents
  keep one, each in a form of its own that is not promised to anyone, and
  naming the session at launch stops Claude Code from making its own.

## Consequences

- Adroit keeps, for each session, whether a person gave its name, so that
  name outlives renaming by the agent and the session being brought back.
- A name the agent chose is the agent's to change again; one a person gave
  is the person's alone.
- Naming takes a call in the agent's turn, and an agent that ignores the ask
  leaves the name made from the prompt.
- The agent's own interface is not told the new name, since telling it means
  typing a command into the agent while it works.
