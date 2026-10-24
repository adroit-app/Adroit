# Keep pasted images apart from the prompt text until sending

Status: Accepted

## Context

Neovim holds text only, and each agent takes images its own way. Image
references mixed into the text cannot show what is attached and tie the prompt
to one agent's syntax. A prompt may be sent long after it is written, after the
clipboard has changed.

## Decision

Pasted images are kept as attachments apart from the prompt text, copied when
pasted, and the receiving agent's adapter prepares them when the prompt is
sent.

## Alternatives considered

- Write a reference to a saved image file into the text. Editing the reference
  changes the attachment, and the text depends on the agent's syntax.
- Forward the clipboard when sending. A later clipboard change would replace
  the image meant.

## Consequences

- Images can be seen and removed without editing the user's words.
- Attachments belong to what the prompt is written for, as its text does.
- Each agent's image input is an interface its adapter depends on.
