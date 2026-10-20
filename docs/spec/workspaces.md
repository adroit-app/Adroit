# Workspaces

adroit lists every Claude Code session by the directory it runs in: the ones
it runs ([details](sessions.md)) and the ones Claude Code reports running in
other terminals. What holds them is what git says about the directory each
runs in.

## The tree

A **workspace** is where sessions and the shells you open in adroit run:

- Scratch, holding every session and shell that runs in the scratch folder or
  below it ([scratch sessions](#scratch-sessions));
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
**picker**. Its first row starts a [scratch session](#scratch-sessions). It
lists every session next, each with where it runs and how it stands, and then
the directories to start a new session in:

- the directories sessions run in now;
- the directories Claude Code has been used in, the most recent first;
- the repositories `ghq list` names, when ghq is installed and the settings
  do not turn it off;
- the folders in each folder the settings list for the picker
  ([settings](settings.md#settingstoml)).

Typing narrows both by a fuzzy match, fzf's syntax included: a session on its
name and where it runs, a directory on its path. The sessions stay above the
directories however well either answers, and the scratch row stays on top
while what is typed matches its words. `Ctrl+n` and `Ctrl+p` (or the arrow
keys, `Tab` and `Shift+Tab`) move, `Enter` chooses, and `Escape` closes the
picker.

Choosing a session opens it as its row in the sidebar would, leaving the keys
where they were. Choosing a directory opens a new session there: the first
prompt sent starts it, named from its first words or from a first line
`name: <label>`. A prompt with no words in ASCII letters, one written in
Japanese for one, is named by its own first letters, cut short.

`Ctrl+f` turns the picker into a **file tree** for a directory the list does
not offer. It shows the directories under the home directory, under each
ghq root and under each folder the settings list, side by side. `j` and `k` move, `l` opens a directory, `h` closes it
or moves to its parent, `!` shows or hides the hidden ones, and `Enter`
chooses the directory under the cursor. `Ctrl+f` goes back to the list and
`Escape` closes the picker.

`n` on a row starts a session in that row's worktree or directory without the
picker. `Cmd+}` and `Cmd+{` open the next and
the previous session without asking.

## Scratch sessions

A **scratch session** is for talking to Claude about something no project
holds, with no directory to choose. `Cmd+Alt+n`, the picker's first row or New
Scratch Session in the Session menu opens one, with the prompt ready as for a
new session, and so does a prompt sent with nothing open when it has no one
worktree to go to ([prompt](prompt.md)). The first prompt sent starts it in
a new directory of its own,
named by the local date and time, in the scratch folder: the one the settings
name, or `$XDG_DATA_HOME/adroit/scratch` (`~/.local/share/adroit/scratch`
when `$XDG_DATA_HOME` is unset). One never sent makes no directory.

The sidebar lists everything that runs in the scratch folder as one
workspace, Scratch, after every other, its sessions and shells straight under
it. Starting a session from it starts another scratch session in
a directory of its own; a shell opened from it runs in the scratch folder.

adroit removes none of these directories. Claude Code keeps each conversation
by the directory it ran in, and the picker offers that directory among those
Claude Code has been used in, so a session started there can `/resume` it.

Claude Code asks whether to trust a folder before a session first works in
it, and each scratch session works in a new one: its console shows the
question until it is answered there. Claude Code takes a folder outside git
as trusted when a folder above it is, so trusting the scratch folder itself,
by running `claude` once in a shell opened from Scratch, keeps the question
from coming back.
