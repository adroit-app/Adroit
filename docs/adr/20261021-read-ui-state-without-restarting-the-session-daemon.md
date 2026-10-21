# Read UI state without restarting the session daemon

Status: Accepted

## Context

The controller and terminal sessions share one daemon. Durable controller history
can outgrow a single response, while replacing that daemon interrupts live work.
The existing daemon has no hot upgrade mechanism.

## Decision

Human UI clients read compact controller pages and fetch full details separately.
For an existing daemon that explicitly cannot serve these reads, the UI may read
its compatible database through a read-only connection. Controller mutations
always use the authenticated daemon endpoint.

## Alternatives considered

- Restart the daemon to upgrade its protocol. This interrupts live terminals.
- Increase the frame limit. Durable history remains unbounded.
- Discard old history. This destroys evidence and correspondence.

## Consequences

- UI recovery does not require replacing the running daemon.
- The compatibility reader must reject unknown schemas and inconsistent pages.
- Preview data cannot be used as mutation authority or completion evidence.
