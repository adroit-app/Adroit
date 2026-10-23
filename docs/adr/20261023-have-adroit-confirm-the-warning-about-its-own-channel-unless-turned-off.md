# Have Adroit confirm the warning about its own channel unless turned off

Status: Accepted; supersedes
20261022-let-the-user-have-adroit-confirm-the-warning-about-its-own-channel.md.

## Context

Claude Code loads Adroit's channel through its development-channel opt-in,
then warns at every launch and waits for confirmation. Every Claude Code
session Adroit starts now carries the channel, because any of them can come to
receive messages, so every one of them stops at the warning until someone
opens its terminal.

## Decision

Adroit confirms that warning once per launch unless the user turns this off
in the settings, and only when the warning names Adroit's own channel and
nothing else. Every other dialog, including a warning that names another
channel, is left to the user.

## Alternatives considered

- Confirm it only when the user turns it on. Every session, including the ones
  sessions start while no one watches, would wait on the user.
- Load the channel only for sessions that come to receive messages. Claude Code
  loads channels at launch, and any session can be sent a message later.
- Confirm any development-channel warning. A channel someone else added to the
  launch would be accepted in Adroit's name.

## Consequences

- Sessions start without a visit to their terminal unless the user has turned
  confirmation off.
- With it off, the warning is reported as waiting for the user, like any other
  dialog.
- A warning whose wording changes is left to the user until Adroit recognises
  it again.
