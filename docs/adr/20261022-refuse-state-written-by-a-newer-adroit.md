# Refuse state written by a newer Adroit

Status: Accepted

## Context

The work host and the command line open the same stored state, and need not be
the same build: a work host keeps running until it is replaced, and the command
line is whatever build was installed last. A build that meets a record written
by a newer one cannot read it, and once reported the whole store as damaged.

## Decision

The stored state's schema version covers the shape of every record, not only
its tables. A build that finds a newer version than its own refuses the state,
on every request and not only on opening, writes nothing to it, and says so
apart from damage.

## Alternatives considered

- Let older builds ignore fields they do not know. Writing a record back drops
  what the newer build recorded, silently.
- Check the version only on opening. A work host opens once and outlives the
  build that started it, which is the case that fails.

## Consequences

- Changing a stored record's shape needs a new schema version.
- An older work host still running stops serving that state until it is
  replaced.
- A newer build reads and upgrades what an older one wrote.
