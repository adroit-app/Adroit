---
name: start-session
description: Hand a self-contained piece of work to a new Claude Code session in adroit, which the user can watch and steer, instead of to an agent inside this session. Use when the user asks to open, start or hand work to a session in adroit, or for a long task the user would want to follow and step into.
---

# Start a session in adroit

This session runs in adroit, a window that shows many Claude Code sessions
side by side. A session started here is a full session of its own: the user
reads it in its own console, writes to it, and can take it over. Prefer it to
an agent inside this session when the work is long, when the user may want to
step in, or when the user asks for it. Keep to an agent for a quick lookup
whose answer is all you need.

## Start one

```sh
@ADROIT@ session start --dir <directory> --name <name> -- "<prompt>"
```

- `--dir` is where it works; it defaults to the current directory.
- `--name` names it; without one it is named from its prompt.
- `--read-only` starts it to read only, in plan mode without the editing
  tools.
- With no prompt after `--`, the prompt is read from standard input, which
  suits a long one:

  ```sh
  @ADROIT@ session start --name review-api <<'EOF'
  ...
  EOF
  ```

The command prints the name the new session goes by.

## Hear back from it

The new session knows nothing of this conversation. Its prompt has to carry
everything it needs: the goal, the context, and what to hand back.

Sessions on this machine message each other by name with `SendMessage`. Find
this session's own name in the first line `ListAgents` prints, and end the
prompt with an instruction such as:

> When you are done, send what you found to `<this session's name>` with
> SendMessage.

Its message arrives here when it is sent; there is no need to wait or poll.
To ask it something meanwhile, `SendMessage` to the name the command printed.

## Do not

- Do not start a session for every small step. Each one is a session the user
  has to keep an eye on.
- Do not start a session from a session you were started from to do the same
  work again.
