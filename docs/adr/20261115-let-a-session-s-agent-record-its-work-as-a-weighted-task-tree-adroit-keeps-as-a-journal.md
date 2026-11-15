# Let a session's agent record its work as a weighted task tree Adroit keeps as a journal

Status: Accepted

## Context

How far a session's work has come is known only inside its agent: Claude Code
keeps a todo list, Codex a plan, Pi and OpenCode something else or nothing,
each in its own interface and gone with the conversation. From the window the
user sees that a session runs, not what it has done of what it set out to do.
A flat list also says nothing of how large each step is, so a count of the
steps done misstates the progress. Every agent Adroit starts already carries
Adroit's own MCP server.

## Decision

A session's agent records its plan and progress through Adroit's own tools as
a tree of tasks, each with a weight among its siblings and a status. The work
host keeps, for each session, a journal of every call in the order made, and
everything shown of the tree, its progress and each task's history is rebuilt
by replaying it. Siblings split their parent's share of the work by weight,
so splitting a task into subtasks leaves its share unchanged.

## Alternatives considered

- **Read each agent's own todo list.** Only some agents keep one, each in a
  form that is not promised to anyone, with no weights, and it ends with the
  conversation.
- **Keep only the tree as it stands.** The history of a task, and when a tree
  was begun, would be lost, and a change that went wrong could not be told
  from the state it left.
- **Count the tasks completed.** A small step and a large one would weigh the
  same, and splitting a task would move the progress.

## Consequences

- The tree is the same for every agent, and outlives the conversation that
  recorded it.
- Recording takes calls in the agent's turn; an agent that does not record
  leaves no tree, and the user sees none.
- The journal only grows while the session is kept. A change that cannot be
  made is skipped, the same way on every replay.
- What is shown can be changed later and computed afresh from the journal of
  the past.
