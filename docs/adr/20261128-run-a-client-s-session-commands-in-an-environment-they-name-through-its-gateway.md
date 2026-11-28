# Run a client's session commands in an environment they name, through its gateway

Status: Accepted

## Context

A program on a client computer, such as an assistant that orchestrates
sessions, could start and follow sessions only on the machine it runs on.
Work on a paired work machine was reachable from the window alone; the
client's commands were refused while a work machine was selected, so the
orchestration had to run on the work machine too. The work machine already
serves paired clients through its gateway, which relays both hosts' sockets
as they are and admits a change only by an ID it keeps.

## Decision

A session or terminal command names the environment it runs in, apart from the
instance's own, and reaches a paired work machine's through its gateway like
the window does. It runs there as a paired client's command: a change is sent
once, under an ID, and an answer lost after it was sent is reported as unknown
rather than asked again. A command that only reads is no change and can simply
be asked again. Paths are the work machine's.

## Alternatives considered

- Leave it to the user to run the commands on the work machine, over SSH or
  the like. Every client would need a login there besides its pairing, and
  the orchestration would no longer run where the user works.
- Redirect the client instance to the work machine. Every window and command
  of the instance would change environment together, which an open
  connection must never do.
- Journal a command's changes as the window does, to recover their outcomes
  later. A command ends with its answer; it has no one to show a recovered
  outcome to, and reporting it as unknown hands the check to the program that
  asked.

## Consequences

- An orchestrating program can tell a change that failed from one that may
  have happened, and has to check before asking for the latter again.
- A watch that follows sessions on a work machine resumes across a lost
  connection, as its reads are asked again.
- A path relative to the client's folder means nothing on the work machine,
  so such a path is refused rather than read against another folder.
- The keys that let a program answer and end sessions stay with the work
  machine's environment.
