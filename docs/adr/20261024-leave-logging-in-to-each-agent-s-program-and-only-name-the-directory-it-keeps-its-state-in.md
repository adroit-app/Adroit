# Leave logging in to each agent's program and only name the directory it keeps its state in

Status: Accepted

## Context

The agents' programs log in themselves, keep their credentials in their
state directory or in the system's keychain under a name drawn from that
directory, and renew them on their own. How they store them differs between
programs and changes between releases.

## Decision

adroit stores, copies and reads no credentials. A connection names the
directory an agent's program keeps its state in, fixed when the connection
is made; adroit starts the program there and leaves logging in, and keeping
the login, to it. The agent's own environment names no directory, so adroit
sets nothing for it.

## Alternatives considered

- Keep the credentials in adroit and hand them to each session. adroit would
  hold secrets, follow every program's renewal, and break when a program
  changes how it stores them.
- Copy an existing login into a new connection. The copy goes stale when the
  program renews the original, and a secret is duplicated.
- Let a connection's directory follow its name. The programs tell logins
  apart by the directory, so a rename would lose the login.

## Consequences

- Settings, session records and anything else adroit writes hold no secrets.
- A connection is only as separate as its program keeps its directory;
  that has to be known of each program, not assumed.
- Moving a connection's directory by hand loses its login.
