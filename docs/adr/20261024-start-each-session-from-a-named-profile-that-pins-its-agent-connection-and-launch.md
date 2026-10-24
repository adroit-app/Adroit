# Start each session from a named profile that pins its agent, connection and launch

Status: Accepted

## Context

adroit kept one launch and one set of options for each agent. A user with a
personal and a work account of the same agent, or with one agent set up for
different jobs, could not run them side by side: changing the settings
changed every session started after, and nothing said how a running session
had been started.

## Decision

A session starts from a named profile, which pins the agent, the connection
the agent runs in, how it is launched and the agent's options. A connection
is one state directory of an agent's program, shared by the profiles that
name it; every agent also has its own environment, the program as it runs
outside adroit, in which adroit changes nothing. Profiles and connections
are referred to by ids that a rename leaves alone. A session keeps the
profile and connection it started from.

## Alternatives considered

- Keep one launch per agent and switch it in the settings. Two accounts
  still cannot run at once, and a session does not know what it ran with.
- Hold the state directory in each profile. Two profiles of one account
  would each need it set alike, and could drift onto different logins.
- Refer to profiles by name. A rename would cut every session and record
  from the profile it started from.
- Make each account an agent of its own. Every adapter would be duplicated
  for each account.

## Consequences

- One agent can run under several logins at the same time.
- Whatever adroit keeps or asks per agent, such as model lists, histories
  and past conversations, has to be kept and asked per connection.
