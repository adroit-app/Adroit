# The prompt

The prompt under the console is where messages to the shown tab are written,
in Neovim. Ctrl+Enter sends what it holds to that tab: a session gets it as
its next message, a shell as a line, and a draft starts its session with it.
The prompt is left empty, in Neovim's normal mode.

## What is written stays with its tab

What is written in the prompt belongs to the tab shown while it was written.
Showing another tab puts it aside and brings back what was written for that
one; nothing written for one session is sent to another by switching tabs.
What is written with nothing open becomes the first prompt of a new session
opened next. Sent with nothing open, it starts a session in the worktree
last opened, or in the only one declared; with more to choose from, adroit asks
for one to be opened.

What a session's tab kept follows the session when it goes by another id,
as after a take-over. When the shown tab closes, a shell closed or a session
gone, what the prompt holds stays, for the tab shown next, before what that
tab kept.

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

`/` at the start of a word offers Claude Code's commands, the account's own
skills and plugins among them, as Claude Code lists them for where the shown
session runs or a draft would start, narrowed by what follows the `/`. Until
Claude Code has answered there, or when it cannot, `/` offers nothing. The list
is asked for again once it is a minute old, the older one offered meanwhile. A
shell's tab offers no commands.

## A shell line

A prompt whose first character is `!` is a shell line for Claude Code, and the
card shows it as one, except on a shell's tab, where a `!` is text like any
other. Sent to a session, it runs in Claude Code's bash mode, as if typed there,
and its output joins the conversation. A draft does not take a shell line; its
session is started with a message first.
