# Choose session orchestration at launch

Status: Accepted

## Decision

A human session can run independently or participate in Leader / Worker
orchestration. Its launch choice determines whether it receives controller
ownership and role policy.

## Alternatives considered

- Make every human session a Leader. Ordinary interactive work then pays for
  coordination even when the user does not want to delegate.
- Remove orchestration entirely. Users lose supervised delegation when they
  do need several sessions to work together.

## Consequences

- Ordinary sessions can start without a functioning controller.
- Switching navigation views does not change a running session's ownership.

## Context

Users need both ordinary interactive sessions and coordinated work. Treating
every new session as a Leader makes the ordinary case depend on orchestration.
