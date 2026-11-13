# Let an environment added join one its work host already serves

Status: Accepted

## Context

Adding an environment always made a new one on its work host. A computer that
paired with a machine saw none of the work already running there, even work it
had started itself from another computer, so the user could not carry work from
one computer to another. Environments are named on each computer, so the work
host held nothing to tell one of its environments from another by.

## Decision

The work host lists the environments it serves, its own among them, and an
environment added either joins one of them or makes a new one. Every window
that joined an environment works in the same sessions and work settings. The
work host keeps, for each environment, the name a window last called it, to
offer whoever joins it next; each computer still names its environments
itself.

## Alternatives considered

- Join the work host's own environment whenever a machine is added. Work made
  in any other environment there could not be reached, and a second
  environment could not be made.
- Keep the name of an environment on the work host alone. Two computers could
  not call one environment differently, and every rename would race the other
  computers' windows.
- List environments with nothing but their identity. The user could not tell
  which one holds the work to go on with.

## Consequences

- Several windows, on several computers, can drive one environment at once.
- An environment's name on the work host is a hint, never an identity: it
  does not decide which environment a window joins or what it may do there.
- An environment added again is found by its machine and the environment it
  works in there, not by its machine alone.
