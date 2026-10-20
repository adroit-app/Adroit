# The prompt

The prompt under the console is where messages to the shown session or
draft are written, in Neovim. Ctrl+Enter sends what it holds to that tab: a
session gets it as its next message, and a draft starts its session with it.
The prompt is left empty, in Neovim's normal mode. Sent
empty, it passes Ctrl+Enter itself to the session, as Claude Code's own
input would take it; a draft gets nothing from an empty prompt.

A shell's tab shows no prompt: its console takes the room and every key, and
the keys that would go to the prompt stay with the shell.

## What is written stays with its tab

What is written in the prompt belongs to the tab shown while it was written.
Showing another tab puts it aside and brings back what was written for that
one; nothing written for one session is sent to another by switching tabs.
A tab with something set aside for it is marked, and so is its session's row.
What is written outlives the window: opened again, each tab gets back what
was written for it, all but a new session's that was never started.
What is written with nothing open becomes the first prompt of a new session
opened next. Sent with nothing open, it starts a session in the worktree
last opened, or in the only one declared; with more to choose from, or none, it
starts a [scratch session](workspaces.md#scratch-sessions).

What a session's tab kept follows the session when it goes by another id,
as after a take-over. When the shown tab closes or its session is gone, what
the prompt holds stays, for the tab shown next, before what that tab kept.

Text taken from Claude Code's input goes to the tab it was taken for, even
when another tab is shown by the time it arrives. A message that cannot be
sent goes back to its tab.

While the shown session waits on an answer, the prompt's keys and text are
that answer, sent to the session; what the prompt holds stays for after.

## What it offers as it is written

`@` at the start of a word offers what it can name: the sessions; the entries
of the open worktree, walked a directory at a time, or from the root of the
disk when the path starts with `/`; and the files git tracks there, found by
any part of their path. What follows the `@` narrows the offer. Picking a
directory goes on to offer what is in it. Hidden entries are not offered but
can be typed.

`/` at the start of a word offers the shown session's agent's commands,
narrowed by what follows the `/`. A Codex session or draft offers Adroit's
catalog of Codex built-in commands; availability in Codex depends on the
installed release and its configuration. Custom Codex commands and skills
are not included in this catalog.

A Claude Code session or draft offers Claude Code's commands, the account's own
skills and plugins among them, as Claude Code lists them for where the shown
session runs or a draft would start. Until
Claude Code has answered there, or when it cannot, `/` offers nothing. The list
is asked for again once it is a minute old, the older one offered meanwhile.

## A shell line

A prompt whose first character is `!` is a shell line for Claude Code, and the
card shows it as one. Sent to a session, it runs in Claude Code's bash mode,
as if typed there, and its output joins the conversation. A draft does not
take a shell line; its session is started with a message first.

## A pasted image

An image pasted into the prompt, with no text beside it on the clipboard, is
saved by adroit, and the prompt names it at the cursor in the form Claude Code
attaches as an image when the message is sent. Claude Code takes PNG, JPEG,
GIF and WebP images; another kind is refused with a notice. A video is not
taken, as Claude Code reads none. A saved image is kept for a week after it is
pasted, so a prompt kept longer names an image that is gone.

Where the console's program has the text instead, as in a shell or while
Claude Code is answered from the prompt or passed every key, the program is
handed an empty paste, as a terminal hands on an image paste; Claude Code
reads the image off the clipboard itself.
