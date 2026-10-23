---
name: start-session
description: Hand work to a session of your own in Adroit, one the user can watch and step into, or message another session working in the same repository. Use for work that can run apart, runs long, needs its own worktree or another agent.
---

# Work with other sessions in Adroit

Do the work yourself unless it is better done apart. Start a session for work
that can run alongside the rest, that runs long, that needs a worktree of its
own or another agent, or that the user may want to watch. Short side tasks go
to your own subagents.

## Start a session

Call the `adroit` MCP server's `start_session` tool with a complete first
prompt: the goal, the scope, what must be checked and what to report. Set
`worktree: true` when it edits files, so it works apart from you, and `base`
to start from a revision another session left. Choose `agent` and `model`
for the work; `models` lists what each agent accepts.

The session works in its own console, which the user can open and type into.
Its report reaches you as a message between your turns: end your turn rather
than waiting. A report is evidence to check, not proof.

Without the MCP server, the same start is a command:

```sh
@ADROIT@ session start --dir <dir> --worktree -- "<first prompt>"
```

## When you were started by another session

Finish and check the work, then call `report` once with what you did, where
it is, how you checked it and what remains. Adroit ends your session after
that turn. You cannot start sessions: if the work proves larger than it
looked, `send` your starter what you found and how you would divide the rest.

## Sessions that share your repository

Before touching something you share with other sessions — the checkout, a
branch, a port — call `sessions` with `repo` set to your directory and `send`
each of them what you are about to do, and afterwards what you did. Do not
wait for answers or take locks. Send only what the recipient has to act on.
