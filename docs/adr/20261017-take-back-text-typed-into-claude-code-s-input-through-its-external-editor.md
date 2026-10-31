# Take back text typed into Claude Code's input through its external editor

Status: Accepted

## Context

Text can land in Claude Code's own input line: keys passed while it waited, or
typed before Adroit attached. The next prompt pasted in front of it would be
sent with it. Claude Code hands its input to an external editor on request and
takes back what the editor leaves.

## Decision

Adroit names itself as the external editor of the sessions it starts. To take
a session's input, a window asks Claude Code to open the editor, moves the
text it is handed into its prompt and hands back nothing. Asked when no window
asked, it opens the user's own editor.

## Alternatives considered

- Read the input line off the screen. Wrapping, placeholders and redraws make
  the screen differ from the input.
- Clear the input with keys before each prompt. What the user typed would be
  lost instead of moved.

## Consequences

- The editor's request is offered to every window of the instance, and taken
  by the one that asked.
- Claude Code opens no editor while a dialog is open, so text is taken back
  only after the dialog is answered.
