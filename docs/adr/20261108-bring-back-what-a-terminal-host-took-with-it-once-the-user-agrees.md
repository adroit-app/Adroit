# Bring back what a terminal host took with it, once the user agrees

Status: Accepted

## Context

Every session and terminal lives in the terminal host's pseudo-terminals.
When the terminal host goes, restarted, crashed or with the machine, all of
them end at once, and the user rebuilds the work by hand: resuming each
conversation, opening each terminal again, putting each tab back. What the
user lost is the work in progress, not the processes: every agent keeps its
conversation, and resumes it on its own.

## Decision

A session or terminal whose terminal host went is interrupted, not ended:
Adroit tells the two apart by whether the terminal host that held it is the
one running now, and keeps what bringing it back needs. An interrupted
session comes back as a new session of Adroit's own that resumes its
conversation through its agent's own resume, and an interrupted terminal as
a new one in the same directory; each takes the tabs the interrupted one had.
Adroit brings them back once the user agrees, unless the user's settings say
to always or never do so. A conversation a running session already holds is
not resumed again.

## Alternatives considered

- Bring everything back at once without asking. Work moves at the pace of
  the user's attention, and a resumed agent may carry on with what it was
  doing; a crash during a run the user did not mean to repeat would repeat
  it.
- Keep each session's process alive across the terminal host, by handing
  its pseudo-terminal to the next one. A reboot or a crash takes the
  processes with it anyway, and it ties the terminal host's restart to a
  handover every program has to survive.
- Treat every session the terminal host took as ended, and leave resuming
  to the list of past conversations. It is what was there, one
  conversation and one tab at a time.

## Consequences

- Restarting the terminal host ends what it holds and lets the user bring
  it back.
- A session brought back is a new session that only shares the
  conversation; what it scrolled back, and anything its agent did not keep,
  is gone, as is everything a terminal ran.
- A session that ended on its own, or that the user ended, is never brought
  back.
- Only the sessions Adroit runs are listed: an interrupted one is offered to
  be brought back, not shown as running.
