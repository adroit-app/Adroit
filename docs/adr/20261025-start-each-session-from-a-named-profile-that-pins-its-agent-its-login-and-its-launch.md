# Start each session from a named profile that pins its agent, its login and its launch

Status: Accepted

## Context

Adroit kept one launch and one set of options for each agent. A user with a
personal and a work login of the same agent, or with one agent set up for
different jobs, could not run them side by side: changing the settings
changed every session started after, and nothing said how a running session
had been started. A login of an agent's program is the directory it keeps
its state in, and the program as it runs outside Adroit has one already,
shared with the user's terminal.

## Decision

A session starts from a named profile, which pins the agent, where the
agent keeps its login, how it is launched and the agent's options. A
profile's login is either the agent's own environment, the program as it
runs outside Adroit, in which Adroit changes nothing, or a folder of the
profile's own, which no other profile shares. Profiles are referred to by
ids that a rename leaves alone. A session keeps the profile it started from
and the login it ran in.

## Alternatives considered

- Keep one launch per agent and switch it in the settings. Two logins still
  cannot run at once, and a session does not know what it ran with.
- Name logins apart from profiles, so several profiles can share one. A
  profile and the login it names cannot be told apart by those who set them
  up, and what sharing gives, one login run with different models or
  permissions, a session's own model and permissions already give.
- Refer to profiles by name. A rename would cut every session and record
  from the profile it started from.
- Make each login an agent of its own. Every adapter would be duplicated for
  each login.

## Consequences

- One agent can run under several logins at the same time.
- Whatever Adroit keeps or asks per agent, such as model lists, histories
  and past conversations, has to be kept and asked per login: the agent's
  own environment, or one profile's folder.
- Profiles that are to share a login run in the agent's own environment; a
  second login of one's own is a second profile, logged in again.
