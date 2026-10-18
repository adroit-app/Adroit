# Interactive sessions

A session started with `claude` in a terminal, rather than by adroit, is
interactive. adroit lists it with the others, but it shows only in the terminal
it runs in, so the console says where it is instead of showing it, and it
shows no state but working or not.

## Taking one over

Press `t` on an interactive session, in the sidebar or in the console showing
it, to bring it into adroit. adroit asks first; once you agree, Claude Code
exits in its terminal and the same conversation continues in a session of
adroit's own, in the same tab.

- Only a session between turns can be taken over. While it works or waits on
  a question or a permission prompt, `t` says so and stops nothing.
- When the session changed in its terminal while adroit was asking, nothing is
  stopped.
- When Claude Code does not exit, adroit says so and leaves it running.

Its tab follows the session, and so does its row in the sidebar, even while
another tab is shown.
