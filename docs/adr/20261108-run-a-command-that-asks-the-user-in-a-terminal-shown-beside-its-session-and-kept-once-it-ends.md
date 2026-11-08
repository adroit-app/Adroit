# Run a command that asks the user in a terminal shown beside its session and kept once it ends

Status: Accepted

## Context

A session's own commands run with no terminal, so one that asks the user for
something, such as a password for `sudo`, fails. A session could start it in
an Adroit terminal, but the terminal only waited in the tree, and the user had
to find it to type. Once the program ended, the terminal ended with it, and
the session could no longer read what it printed or how it ended.

## Decision

A session runs a command that asks the user in an Adroit terminal it asks to
be shown. The window opens that terminal with the keys only when it appears
while the user looks at the session that started it, as it does for a session
one opens; otherwise it waits in the tree. A command terminal outlives its
program: Adroit runs the program, keeps the screen and records the status it
ended with on the terminal, until the terminal is stopped. Sessions are told to
do this rather than hand the command back to the user.

## Alternatives considered

- Hand the command to the user. They have to copy it to a terminal of their
  own, and the session cannot see what came of it.
- Show the terminal wherever the user is. The keys would leave what the user
  was typing elsewhere.
- Let the session type the command into a shell terminal and read the status
  the shell prints. What the screen shows then depends on the user's shell
  and prompt, and nothing records the status.
- Offer it as a tool on Adroit's MCP server. Every agent can run a command,
  and terminals are already driven by one.

## Consequences

- Every command terminal stays until it is stopped, however its program ended.
- The window learns which session started a terminal.
- A terminal asked to be shown while the user looks elsewhere is only listed;
  the session has to tell the user where it is.
