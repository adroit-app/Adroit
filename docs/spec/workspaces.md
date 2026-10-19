# Workspaces

adroit lists every Claude Code session by the directory it runs in: the ones
it runs ([details](sessions.md)) and the ones Claude Code reports running in
other terminals. What holds them is what git says about the directory each
runs in.

## The tree

A **workspace** is where sessions and the shells you open in adroit run:

- a git repository, holding every session and shell that runs in any of its
  worktrees or in a directory below one;
- otherwise the directory a session or a shell runs in.

Inside a git workspace, each session and each shell sits under the worktree
it runs in, a shell marked as a terminal; a worktree where only shells run is
listed for them. A workspace where everything runs in its main checkout shows
it straight under it, with no worktree row; one outside git always does.

Opening a shell's row shows that shell in the console and moves the keys
there, as opening a session's row does. Closing it from its row ends the
shell, as closing its tab does ([shells](shells.md)). Going from session to
session, the waiting sessions and the picker pass shells over.

`!` in the sidebar shows every worktree git knows of each workspace, those
where nothing runs too, drawn dim; opening one starts a session there.
Pressing it again goes back to the worktrees where sessions or shells run. The
window does not keep this between runs.

Sessions running in other terminals are listed beside adroit's own. A
session that has ended is listed no more; Claude Code keeps its conversation,
which a session started later resumes with Claude Code's `/resume`.

## Going to a session or starting one

`Cmd+p`, `w`, `+` beside Workspaces, or the Session menu opens the
**picker**. It lists every session first, each with where it runs and how it
stands, and then the directories to start a new session in:

- the directories sessions run in now;
- the directories Claude Code has been used in, the most recent first;
- the repositories `ghq list` names, when ghq is installed.

Typing narrows both by a fuzzy match, fzf's syntax included: a session on its
name and where it runs, a directory on its path. The sessions stay above the
directories however well either answers. `Ctrl+n` and `Ctrl+p` (or the arrow
keys, `Tab` and `Shift+Tab`) move, `Enter` chooses, and `Escape` closes the
picker.

Choosing a session opens it as its row in the sidebar would, leaving the keys
where they were. Choosing a directory opens a new session there: the first
prompt sent starts it, named from its first words or from a first line
`name: <label>`.

`Ctrl+f` turns the picker into a **file tree** for a directory the list does
not offer. It shows the directories under the home directory and under each
ghq root, side by side. `j` and `k` move, `l` opens a directory, `h` closes it
or moves to its parent, `!` shows or hides the hidden ones, and `Enter`
chooses the directory under the cursor. `Ctrl+f` goes back to the list and
`Escape` closes the picker.

`n` on a row starts a session in that row's worktree or directory without the
picker, and `N` starts a reader there. `Cmd+}` and `Cmd+{` open the next and
the previous session without asking.

## Sessions that only read

A session started as a reader runs in Claude Code's plan mode with the tools that
edit files denied, for looking into something or reviewing without changing
the worktree under the sessions working in it. It is marked with a book.
