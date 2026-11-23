# Update a work machine's Adroit through its own Homebrew when a paired client asks

Status: Accepted

## Context

A fix to the hosts reaches a work machine only once Adroit is updated there,
and a user away from it could not do that. A paired client already acts with
the authority of the user the machine runs as.

## Decision

A paired client may ask the gateway to update Adroit, saying nothing of what
to. The gateway has the Homebrew that installed the Adroit it runs from
upgrade it, checks that what it installed starts, and only then restarts the
work host and itself. The terminal host is left running, and no session ends.

## Alternatives considered

- Have the client send the program or say where it is. A client would choose
  what the machine installs, and the machine could not tell a release from
  anything else.
- Have Adroit download a release and replace itself. Homebrew would no longer
  know what it installed, and Adroit's releases carry no signature a machine
  trusts by itself.
- A remote shell. It opens a second way into the machine, with keys of its
  own.

## Consequences

- Only an Adroit that Homebrew installed is updated; any other is refused.
- What is installed is what the tap and the checksum it carries say.
- An update that does not start is reported, and the hosts already running go
  on; nothing puts the version before it back.
- A gateway that is not run by the service manager goes on as it was until it
  is started again.
- A gateway from before this decision closes the request unanswered, so the
  first update is made at the machine.
