# Run only OpenCode's second line, each interface with a server of its own

Status: Accepted

## Context

OpenCode ships as two lines with different packages, storage, servers and
conversation ids, both maintained. Only the second lets a program choose a
new conversation's id before it is made, tells its logins in a form a program
can read, and can run a server for one interface alone. Its interface and its
other commands otherwise talk to a background server shared by every client
on the machine, which outlives them, takes a port fixed for the machine and
keeps the environment of the client that started it first: one account's
login, or one session's credential, would serve every session after it.

## Decision

adroit runs OpenCode of its second line alone, and refuses to start a session
of an older release, saying why and what to install. Every interface and every
command adroit starts runs with a server of its own, which ends with it.

## Alternatives considered

- Drive both lines, told apart by their version. Logins, servers,
  conversation ids and what adroit loads into a run would each be done twice,
  and the first line's users can move to the second, which takes their
  conversations over.
- Let OpenCode use its background server. Two accounts, or two sessions of
  one, would share one server and its environment.
- Run a server of adroit's own for each session or account and attach the
  interface to it. adroit would start, watch and end servers outside the
  terminal host and keep a password for each, for what a server of the
  interface's own already gives.

## Consequences

- A session's server holds its account's login and its own credential, and
  no other session's.
- Nothing outside a session's interface can reach its server; what adroit
  hands a running session goes through what it loads into the run.
- Asking OpenCode anything outside a session, its version included, starts a
  server, so adroit asks it only as a session of that profile would run, and
  as rarely as it can.
- A release of OpenCode's second line that drops what adroit relies on
  leaves its sessions unable to start until adroit follows.
