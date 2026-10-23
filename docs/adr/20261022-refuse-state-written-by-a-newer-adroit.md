# Refuse state written by a newer adroit

Status: Accepted

## Context

The window, the command line and the session daemon open the same stored state,
and need not be the same build: the daemon keeps running after adroit is
replaced. A build that meets a record written by a newer one cannot read it,
and once reported the whole store as damaged.

## Decision

The stored state's schema version covers the shape of every record, not only
its tables. A build that finds a newer version than its own refuses the state,
on every request and not only on opening, writes nothing to it, and says so
apart from damage.

## Alternatives considered

- Let older builds ignore fields they do not know. Writing a record back drops
  what the newer build recorded, silently.
- Check the version only on opening. A daemon opens once and outlives the build
  that installed it, which is the case that fails.

## Consequences

- Changing a stored record's shape needs a new schema version.
- An older daemon still running stops serving that state until it is
  restarted.
- A newer build reads and upgrades what an older one wrote.
