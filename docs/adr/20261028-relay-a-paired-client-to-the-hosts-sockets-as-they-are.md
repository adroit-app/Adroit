# Relay a paired client to the hosts' sockets as they are

Status: Accepted

## Context

A paired client has to reach the work host and the terminal host, which listen
only on Unix sockets of their own. The window reaches them through forwarded
sockets, and checks their identities, generations and protocols there. A paired client acts with the authority of the user the
work machine runs as.

## Decision

A gateway on the work machine relays each of a paired client's connections to
the socket of the host it asks for, and the client shows them to the window as
forwarded sockets. The gateway does not reinterpret either protocol; it drops
only a request to stop the work host.

## Alternatives considered

- Expose a typed subset of each protocol through the gateway. Every request
  and terminal operation would need a decision and an adapter, and a client
  that is already trusted with the user's authority would gain no protection.
- Expose the hosts' sockets on the network directly. They would have to
  authenticate clients themselves, and a terminal host replaced by its work
  host could not keep its address.

## Consequences

- The window, the mutation journal and the generation checks see the hosts'
  own sockets and need nothing of the gateway.
- Paired clients are not a security boundary inside the work machine; work
  that must be kept apart runs as another user or on another machine.
- A new request in the work protocol reaches the work host through the gateway
  without a change to it.
