# Report a session held at a dialog as waiting on the user

Status: Accepted

## Context

A session's terminal can be running while its agent waits on a dialog:
trusting a directory, a permission, or a warning shown at every launch. The
user, and a session that opened it, would see it as started. Claude Code loads Adroit's
channel through its development-channel opt-in and warns about it at every
launch, and every session Adroit starts carries that channel.

## Decision

A session's readiness is kept apart from its process. The agent's adapter reads
its screen, and any dialog, named or not, counts as waiting on the user. Adroit
answers no dialog for the user, with one exception it makes unless turned off:
the warning that names Adroit's own channel and nothing else.

## Alternatives considered

- Treat a live terminal as ready. It hides the decision that keeps the session
  from working.
- Report only dialogs the adapter recognises and leave the rest to a stall
  timeout. The user learns late, and wrongly, that the start stalled.
- Accept startup dialogs. They are security decisions that belong to the user.
- Confirm the channel warning only when the user turns it on. Every session,
  including those started while no one watches, would wait.
- Confirm any channel warning. A channel someone else added would be accepted
  in Adroit's name.

## Consequences

- Readiness is visible without changing who owns the session.
- A warning whose wording changes is left to the user until Adroit recognises
  it again.
