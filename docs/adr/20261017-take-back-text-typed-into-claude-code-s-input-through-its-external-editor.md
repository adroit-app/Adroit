# Take back text typed into Claude Code's input through its external editor

Status: Accepted

## Context

Prompts are written in adroit's prompt, but text can still land in Claude Code's
own input line: keys passed to a session while it waited, or typed in a session
before adroit attached to it. Pasted in front of that text, the next prompt
would be sent with it. Claude Code hands its input to the external editor named
by `VISUAL` or `EDITOR` when asked with `Ctrl-g`, and takes back whatever the
editor leaves in the file.

## Decision

adroit names itself as the external editor of the sessions it starts. When it
wants a session's input, it asks with `Ctrl-g`, moves the text it is handed into
the prompt and hands back an empty file. Asked when adroit did not ask, it opens
the user's own editor.

## Alternatives considered

- Read the input line off the session's screen. Wrapping, placeholders and
  redraws make the text on screen differ from the text in the input.
- Clear the input with keys before each prompt. What the user typed there
  would be lost instead of moved.

## Consequences

- One window at a time answers the sessions' editor requests.
- Claude Code does not open the editor while a dialog is open, so text is taken
  back only once the dialog is answered.
- `Ctrl-g` pressed by the user in a session still opens their own editor.
