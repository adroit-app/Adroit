# Carry messages between any two sessions through Adroit

Status: Accepted; supersedes
20261019-let-a-session-start-sessions-through-the-adroit-command.md,
20261019-find-the-sessions-a-session-worked-with-by-their-names-in-its-transcript.md,
20261020-persist-orchestration-independently-of-windows-and-agent-conversations.md
and 20261021-treat-native-transport-acceptance-as-message-receipt.md.

## Context

Sessions that Adroit starts hand work to one another and report back. Claude
Code's messaging between sessions leaves Codex out, addresses a session only
by a name that can change or repeat, and leaves no record but the transcript.
Sessions working on unrelated tasks in one repository also need to tell each
other before they touch what they share: the same checkout, a branch, a port.

## Decision

Adroit carries the messages between the sessions it starts. Any such session
can send to any other, addressed by the session's identity rather than its
name. The session daemon keeps every message and which session started which,
so both outlive the window and the sessions themselves. A message is handed
to the recipient's agent through that agent's own means of hearing from
outside; handing it over is receipt, and no acknowledgement is asked for.

## Alternatives considered

- Keep Claude Code's messaging. Codex sessions could not take part, and a
  renamed or repeated name reaches the wrong session.
- Let sessions message only the session that started them and those they
  started. Unrelated sessions sharing a checkout could only coordinate through
  the user.
- Have sessions take locks or wait for every peer's answer before touching
  something shared. One session that does not answer would stop unrelated
  work; working in separate worktrees removes most contention, and a notice
  covers the rest.

## Consequences

- Sessions Adroit did not start can neither send nor receive.
- A message wakes its recipient's agent and costs it a turn, so sessions are
  asked to send only what the recipient has to act on.
- Who talked to whom is read from Adroit's record, not from transcripts.
- A report stays readable after the session that sent it and the one it was
  sent to have ended.
