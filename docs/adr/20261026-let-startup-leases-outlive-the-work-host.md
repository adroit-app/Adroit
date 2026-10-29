# Let startup leases outlive the work host

Status: Accepted

## Context

A new terminal waits for its colours before running its program. The work
host can disappear at any point after that terminal has been created.

## Decision

The terminal's startup gate owns its deadline independently of the work host.
Release reaches the gate through a channel that closes before the program
runs, separate from the program's terminal input.

## Alternatives considered

- Keep the deadline in the work host. Its crash could leave the terminal
  waiting indefinitely, including before the host records the new terminal.
- Expire the gate independently but release it by typing a line. A release
  racing with expiration could send Enter into the running program.

## Consequences

- Losing the work host cannot leave an otherwise live terminal waiting
  indefinitely for its program to start.
- Repeated or late release does not type into the program.
