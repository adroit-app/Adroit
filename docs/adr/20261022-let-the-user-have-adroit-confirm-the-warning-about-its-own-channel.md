# Let the user have Adroit confirm the warning about its own channel

Status: Accepted

## Context

Adroit's control channel is not on Claude Code's list of approved channels, so
Claude Code loads it through its development-channel opt-in. Claude Code then
warns at every launch and waits for confirmation. Every Leader and Worker
stops there until someone opens its terminal.

## Decision

When the user enables it in the settings, Adroit confirms that warning once per
launch, and only when the warning names Adroit's own channel and nothing else.
It is off unless enabled. Every other dialog, including a warning that names
another channel, is left to the user.

## Alternatives considered

- Load the channel as an approved one. Adroit is not on the list, so Claude
  Code refuses it.
- Always confirm the warning. The warning guards against channels the user did
  not choose; confirming it unasked removes that check for everyone.
- Confirm any development-channel warning. A channel someone else added to the
  launch would be accepted in Adroit's name.

## Consequences

- With the setting on, sessions start without a visit to their terminal.
- With it off, the warning is reported as waiting for the user, like any other
  dialog.
- A warning whose wording changes is left to the user until Adroit recognises
  it again.
