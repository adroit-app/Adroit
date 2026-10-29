# Pair a client by a code it shows and the operator types on the work machine

Status: Accepted

## Context

Before two machines can pin each other's keys, each has to learn the other's
key over a network neither trusts yet. Whoever can reach the work machine can
ask to pair with it.

## Decision

The client shows a short one-time code, and the operator types it on the work
machine to approve that request. Both sides prove they know the code through
SPAKE2, whose confirmation is bound to the TLS connection it runs in, so the
code itself never crosses the network. A code is tried at most once, and the
work machine does no exchange for a request before the operator approves it.

## Alternatives considered

- Compare fingerprints by eye. People confirm long strings without reading
  them.
- Send the code, or a hash of it. A machine in between, or one that recorded
  the exchange, could learn or guess a short code offline.
- Approve on the work machine without a code. Whoever asked last would be
  approved along with the one the operator meant.
- A code the work machine shows and the client types. Approval would no longer
  need anyone at the work machine.

## Consequences

- Pairing needs someone at the work machine, and a request that is not
  approved there pairs nothing.
- A relay that terminates TLS on either side fails the confirmation even with
  the right code.
- The exchange relies on a SPAKE2 implementation that has not been
  independently audited; a flaw in it is reachable only while pairing is open
  and the operator approves the attacker's request.
