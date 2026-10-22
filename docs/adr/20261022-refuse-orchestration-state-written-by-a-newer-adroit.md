# Refuse orchestration state written by a newer adroit

Status: Accepted

## Context

The window, the control command and the session daemon all open the same
control store, and they need not be the same build: the daemon keeps running
after the application is replaced. Records are strict about their fields, so a
build that meets a record written by a newer one cannot decode it. The store's
schema version only moved with its tables, and a daemon checked it once, when
it opened the store. A newer window that wrote one attempt with a new field
made an older daemon report the whole store as corrupt and stop scheduling.

## Decision

The store's schema version covers the shape of every record it holds, not
only its tables. A build that finds a version newer than its own refuses the
store, on every request and not only when opening it, and writes nothing to
it. The refusal is its own error, distinct from a damaged store.

## Alternatives considered

- Let older builds ignore fields they do not know. An older build that writes
  such a record back drops what the newer one recorded, and nothing reports
  the loss.
- Keep checking the version only on opening. A daemon opens the store once and
  outlives the build that installed it, which is the case that fails.

## Consequences

- Changing what a stored record holds requires a new schema version, even when
  no table changes.
- Once a newer build has upgraded the store, an older daemon still running
  stops serving orchestration until it is restarted.
- A newer build still reads and upgrades a store written by an older one.
