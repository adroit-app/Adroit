# Name a pasted image in the prompt by a file adroit saves

Status: Accepted

## Context

An image on the clipboard could not be pasted into the prompt: Neovim holds
text only, and the prompt reaches Claude Code as a bracketed paste of that
text. Claude Code takes an image three ways: it reads the clipboard itself
when its input is pasted into with nothing, it loads a pasted line that is
only an image's path, and it attaches an image file a message mentions with
`@`. The first two happen while the input is edited, not when the message is
sent, and a line that is only an image's path makes Claude Code load the file
before it takes an Enter, dropping one that comes meanwhile. A prompt may be
written long before it is sent, in another tab, after the clipboard has
changed.

## Decision

An image pasted into the prompt is saved as a file of adroit's own, and the
prompt holds a quoted `@` mention of that file, which Claude Code attaches
when the message is sent.

## Alternatives considered

- Forward the paste to Claude Code's input when the prompt is sent. Claude
  Code would read whatever the clipboard holds by then, and the prompt would
  show nothing of the image while it is written.
- Write the file's bare path in the prompt. Sent as a line of its own, it makes
  Claude Code hold back the Enter that sends the message; inside a sentence,
  the model has to read the file with a tool, which may ask for permission.
- Keep the image in adroit and type it into Claude Code's input separately at
  sending. Each image would need its own paste, timed against Claude Code
  loading the one before.

## Consequences

- What the prompt holds is plain text, so it is set aside, kept and sent like
  any other prompt, and the image goes with it.
- The files outlive the paste and are removed after a while; a prompt kept
  longer than that names an image that is gone.
- Claude Code's `@` mention, and the kinds of image it attaches, become an
  interface adroit depends on.
