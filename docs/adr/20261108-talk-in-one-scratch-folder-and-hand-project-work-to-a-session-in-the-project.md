# Talk in one scratch folder and hand project work to a session in the project

Status: Accepted

## Context

Talking to an agent about something no project holds needs no directory, but
the agent still runs in one: it reads what is there, keeps the conversation by
it, and asks the user to trust it the first time. Such talk often turns to a
project, and work on a project belongs in its own directory, where the
agent reads its files and the user finds the session among the project's.

## Decision

Every session started without a directory runs in one folder Adroit keeps in
the user's data directory, listed as one place. A session there is told to
keep to talk that belongs to no project: once the talk turns to work on a
project, it opens a session in that project's directory with what that
session needs, shown to the user in its place, and leaves the work to it.

## Alternatives considered

- A new directory for each such session. Each asks again to be trusted, and
  the directories pile up, while the files they would keep apart are few once
  project work leaves.
- A temporary directory. The system clears it, and the conversation can no
  longer be resumed.
- The home directory. The agent would read everything under it.
- Let such a session work on a project from where it is. It works outside the
  project's directory, and the session is not found among the project's.

## Consequences

- The folder is trusted once, and no such session asks again.
- Such sessions share the folder and whatever they leave in it.
- Project work started from such talk runs in a session of the project, which
  starts from what the first session hands it rather than its whole
  conversation.
