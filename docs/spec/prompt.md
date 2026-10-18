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
