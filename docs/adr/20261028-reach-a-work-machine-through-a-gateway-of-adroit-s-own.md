# Reach a work machine through a gateway of Adroit's own

Status: Accepted

## Context

A window on one machine can work on another, whose hosts listen only on Unix
sockets of their own. Something has to carry a client to those sockets across
the network, and it decides what the user has to set up on both machines and
whether a work machine whose address changed is still found.

## Decision

A work machine runs a gateway of Adroit's own that a client pairs with once
and then reaches its hosts' sockets through. Adroit carries no other
transport.

## Alternatives considered

- Reach the sockets over the operating system's SSH. The user would run an SSH
  server on the work machine and keep an entry for it in their SSH
  configuration, which Adroit would have to read and police itself, and a
  machine whose address changed would no longer be found.
- Offer SSH beside the gateway. It reaches nothing the gateway does not, and
  every check on the way to the hosts would be kept and tested twice.

## Consequences

- A work machine has to run the gateway, and each client has to pair with it
  once.
- Reaching a work machine through a jump host is left to tunnelling the
  gateway's port outside Adroit.
