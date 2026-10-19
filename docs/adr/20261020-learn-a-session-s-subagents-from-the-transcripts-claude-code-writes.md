# Learn a session's subagents from the transcripts Claude Code writes

Status: Accepted

## Context

A session that hands work to subagents shows only a line or two of each in
its console, and that scrolls away. What each was asked, whether it is still
at work, and what it is doing are lost to the user. Claude Code offers no API
to ask, but it writes the call that starts a subagent and the notice that it
ended into the session's transcript, and keeps a transcript of each subagent
beside it.

## Decision

The subagents a session runs, and how each stands, are read from the
session's transcript; what each is doing is read from its own transcript.

## Alternatives considered

- Install Claude Code hooks that report subagents starting and stopping.
  It means writing into the user's Claude Code settings, and sessions started
  before the hooks, or outside adroit, would tell nothing.
- Read the output files Claude Code keeps for background tasks. They lie in
  a temporary directory, exist only for subagents run in the background, and
  do not say when one ended.
- Read the console. A subagent's prompt never appears whole there, and what
  scrolled off is gone.

## Consequences

- How Claude Code records subagents, which it does not promise, becomes an
  interface adroit depends on; a change to it can empty the list or leave a
  subagent shown running without an error.
- A subagent whose session's Claude Code quit while it ran has no notice that
  it ended, and stays shown running until one is written.
- Only sessions with a conversation have subagents; shells have none.
