# Pass every key while Claude Code shows a dialog

Status: Accepted; supersedes in part
20261017-pass-only-control-keys-from-the-console-to-a-session.md

## Context

The console passes a session every key only while the session reports it is
waiting on the user. Claude Code reports nothing for the dialogs it opens
itself, such as the picker `/model` opens, so the console kept their arrows,
`Enter` and digits, and a dialog could be operated only after asking for every
key by hand.

## Decision

The console passes every key to a session while Claude Code shows a dialog of
its own, told from what the console shows.

## Alternatives considered

- List the slash commands that open a dialog. It misses dialogs opened
  otherwise, drifts with each release, and cannot tell when the dialog closes.
- Wait for Claude Code to report a dialog. It has no hook for one.
- Leave it to the explicit request for every key. It is manual and easy to
  forget.

## Consequences

- Detection depends on how Claude Code draws its dialogs and its input box.
  A redesign can stop it, which falls back to the earlier behaviour: the keys
  are not passed, and the explicit request still passes them.
- What a session shows on screen, not only what it reports, now decides where
  keys go.
