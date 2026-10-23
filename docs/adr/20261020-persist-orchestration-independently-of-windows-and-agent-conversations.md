# Persist orchestration independently of windows and agent conversations

Status: Superseded by 20261023-carry-messages-between-any-two-sessions-through-adroit.md

## Context

Terminal and provider records describe running conversations. They do not
record a task's revision, which report is current, whether a message was
processed, or the result of a control request whose caller disconnected.
Workers can finish while their leader or window is absent.

## Decision

Persist orchestration independently of windows and agent conversations.
Keep each instance's request ownership, work and pending messages in a
durable control store. Save each state change with its event and replayable
request response as one transaction.

## Alternatives considered

- Reconstruct coordination from terminal or conversation history. A turn
  ending does not establish task success or acknowledge a particular message.
- Keep coordination in the window. Closing or restarting it would lose work
  that its agents continue to perform.
- Save state and notifications separately. A crash between them would leave
  either a missing notification or an event for a change that never happened.

## Consequences

- A request retry can return its stored response without repeating the work.
- Delivery acceptance and an agent's acknowledgment remain distinct.
- An unknown or damaged store fails explicitly instead of becoming empty
  orchestration state.
