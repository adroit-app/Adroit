# Show a session another opens for the user only while the user looks at the opener

Status: Accepted

## Context

A session that opens another for a separate line of work leaves the user where
they were; the new session waits in the tree. When the session hands over the
very talk the user is having with it, the user has to find the new session
to carry on, and a window that moved the focus whenever any session opened
another would take the keys from whatever the user was doing.

## Decision

A session can ask that the session it opens be shown to the user. The window
shows it and gives it the keys only when it appears while the user looks at
the session that opened it; otherwise it waits in the tree like any other.

## Alternatives considered

- Show every session another opens. Sessions open others for work beside
  their own, and the user would lose their place each time.
- Show it whenever asked, wherever the user is. The user may have moved on to
  another session or a file, and the keys would leave mid-typing.
- Leave it to the user. The talk stops until the user finds where it went.

## Consequences

- Asking costs nothing when the user has moved on; the session is only listed.
- A window opened later never shows it for the asking, only as any session.
