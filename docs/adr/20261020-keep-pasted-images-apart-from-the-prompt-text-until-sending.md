# Keep pasted images apart from the prompt text until sending

Status: Accepted; supersedes [Name a pasted image in the prompt by a file adroit saves](20261020-name-a-pasted-image-in-the-prompt-by-a-file-adroit-saves.md)

## Context

Image references mixed into the text cannot show what is attached and tie the
prompt to Claude Code's syntax. A prompt may instead be sent to Codex, whose
terminal input and launch arguments accept images differently.

## Decision

Keep pasted images as attachments apart from the prompt text, and let the
receiving agent's adapter prepare them when sending.

## Alternatives considered

- Keep image references in the text and draw previews for those references.
  Editing a reference would change the attachment, and the text would still
  depend on the receiving agent's syntax.
- Forward the clipboard on sending. A later clipboard change would replace
  the image the user meant to send.

## Consequences

- Images can be inspected and removed without editing the user's words.
- Attachments need the same tab ownership and recovery as the prompt text.
- Each agent's image input is an interface its adapter depends on.
