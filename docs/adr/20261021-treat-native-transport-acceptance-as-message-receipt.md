# Treat native transport acceptance as message receipt

Status: Superseded by 20261023-carry-messages-between-any-two-sessions-through-adroit.md

## Context

A notification starts an expensive model turn. Requiring the model to confirm
receipt creates control calls and replies even when no decision or work changes.
Routine Worker progress has the same cost as a blocking question.

## Decision

Treat native transport acceptance as message receipt. Keep routine Worker
progress in durable state without waking the Leader. Receipt does not establish
processing, task completion, or verified evidence.

## Alternatives considered

- Require a model acknowledgement for every notification. This makes receipt
  itself a task and encourages acknowledgement-only conversation loops.
- Discard routine progress. Humans and Leaders still need to inspect it later.

## Consequences

- Explicit acknowledgement remains available but is not required after delivery.
- Receivers still deduplicate actions by immutable message identity.
- Uncertain transport outcomes still require explicit reconciliation or retry.
