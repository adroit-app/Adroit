# Workspaces

adroit lists every Claude Code session by the directory it runs in: the ones
it runs ([details](sessions.md)) and the ones Claude Code reports running in
other terminals. What holds them is what git says about the directory each
runs in.

## The tree

A **workspace** is where sessions run:

- a git repository, holding every session that runs in any of its worktrees
  or in a directory below one;
- otherwise the directory a session runs in.

Inside a git workspace, each session sits under the worktree it runs in. A
workspace whose sessions all run in its main checkout shows them straight
under it, with no worktree row; one outside git always does.

`!` in the sidebar shows every worktree git knows of each workspace, those
without sessions too, drawn dim; opening one starts a session there. Pressing
it again goes back to the worktrees with sessions. The window does not keep
this between runs.

Sessions running in other terminals are listed beside adroit's own. A
session that has ended is listed no more; Claude Code keeps its conversation,
which a session started later resumes with Claude Code's `/resume`.

## Starting a session

`w`, or `+` beside Workspaces, opens the **picker**, which asks where the
session works. It offers, in one list:

- the directories sessions run in now;
- the directories Claude Code has been used in, the most recent first;
- the repositories `ghq list` names, when ghq is installed.

Typing narrows the list by a fuzzy match on the path, fzf's syntax included.
`Ctrl+n` and `Ctrl+p` (or the arrow keys, `Tab` and `Shift+Tab`) move,
`Enter` chooses, and `Escape` closes the picker.

`Ctrl+f` turns the picker into a **file tree** for a directory the list does
not offer. It shows the directories under the home directory and under each
ghq root, side by side. `j` and `k` move, `l` opens a directory, `h` closes it
or moves to its parent, `!` shows or hides the hidden ones, and `Enter`
chooses the directory under the cursor. `Ctrl+f` goes back to the list and
`Escape` closes the picker.

Choosing a directory opens a new session there: the first prompt sent starts
it, named from its first words or from a first line `name: <label>`. `n` on a row starts one in that row's worktree or
directory without the picker, and `N` starts a reader there.

## Going to another session

`Cmd+p` opens the picker on every session instead, each with where it runs
and how it stands. Typing narrows the list by a fuzzy match on a session's
name and where it runs, the same keys move and close, and `Enter` opens the
session chosen as its row in the sidebar would, leaving the keys where they
were. This picker has no file tree.

`Cmd+}` and `Cmd+{` open the next and the previous session without asking.

## Sessions that only read

A session started as a reader runs in Claude Code's plan mode with the tools that
edit files denied, for looking into something or reviewing without changing
the worktree under the sessions working in it. It is marked with a book.
