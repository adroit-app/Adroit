# Let a session ask to close itself once its task is done

Status: Accepted

## Context

A session whose task was done, or that had handed the rest of it to another
session, sat waiting until the user noticed and closed it. With many sessions
running, finished ones piled up beside the ones still working, and the user
had to read each one to tell which had finished. The user still wanted to be
able to ask a finished session what it did, or have it do the work again,
before letting it go.

## Decision

Every session Adroit starts is told that, once its task is done or handed
off, it shows the user a summary of where things stand and asks whether to
close the session, in the agent's own way of asking. Adroit gives each session
a tool that ends that session and no other, and the agent calls it only after
the user agrees. Closing ends the session as ending it from Adroit does: its
worktree stays and its conversation can be resumed.

## Alternatives considered

- **Asking in the window.** The agent would report that it is done, and the
  window would show the summary with buttons to close or go on. It would look
  the same for every agent, but it needs new state and UI. The agent's own
  question already lets the user answer in words, which is what going on
  means: asking what was done or asking for the work again.
- **Removing the worktree on close as well.** The work may not have landed
  yet, and a removed worktree cannot be resumed into. Removing a worktree stays
  a step of its own that asks first.
- **Closing without asking.** A finished session would vanish before the user
  read its summary or asked about it.

## Consequences

- How the question looks depends on the agent: a question dialog where the
  agent has one, plain words otherwise.
- An agent that misjudges when its task is done asks too early or not at all.
  The instruction is all there is to hold it to.
