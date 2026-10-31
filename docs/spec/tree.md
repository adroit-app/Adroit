# The tree

Adroit lists every session it runs by the directory it runs in
([details](sessions.md)). What holds them is what git says about the directory
each runs in. Sessions started outside Adroit are not listed; their
conversations can be resumed in Adroit
([resuming](sessions.md#resuming-a-past-conversation)).

## Workspaces and worktrees

A **workspace** is where sessions and the shells you open in Adroit run:

- Scratch, holding every session and shell that runs in the scratch folder or
  below it ([scratch sessions](#scratch-sessions));
- a git repository, holding every session and shell that runs in any of its
  worktrees or in a directory below one;
- otherwise the directory a session or a shell runs in.

Inside a git workspace, each worktree where something runs has a row of its
own; a worktree where only shells or Neovim run is listed for them. A
workspace where everything runs in its main checkout shows that checkout's
contents straight under it, with no worktree row; one outside git always does.

A worktree's files sit under its Files heading, and what runs in it under
Sessions and Terminals: Terminals holds every program Adroit runs in a
terminal there, shells and Neovim alike, each marked by what it runs, save a
Neovim started on a file, which that file's row stands for. A heading folds,
and the row of one for what runs counts what it holds. Terminals never sit
straight under a workspace: where a workspace has no worktree row, the
headings are its checkout's.

```text
api                        ← a git repository with sessions in two worktrees
├── main                   ← a worktree
│   ├── Files
│   ├── Sessions
│   │   ├── auth-revamp
│   │   └── review-api
│   └── Terminals
│       └── $ dev-server
└── feat/oauth
    ├── Files
    └── Sessions
        └── implement
notes                      ← a directory outside git: no worktree row
├── Files
└── Sessions
    └── outline-the-talk
Scratch                    ← every scratch session, straight under it
└── explain-regex-flags
```

Files starts folded. Unfolded, it lists the worktree's files as a
[file tree](#file-trees). Hidden files once shown, and what git ignores, are
drawn dim; the version control's own `.git` and `.jj` are not listed. A file jj or
git sees changed is marked at the end of its row with the letter git gives it,
an added one in a jj workspace with `A`, and a directory holding one with a
dot. The marks and the listing follow the disk within seconds.
Opening a file shows it in a viewer or in Neovim, by its kind
([viewers](content.md#viewers)), and its row shows the tabs it is shown in.
A file a Neovim started on it has open is drawn brighter than the other
files, whether a window shows that Neovim or not.

A row of a session, a shell or a Neovim shows the numbers of the content's tabs
it is shown in, and the row of what the focused window shows stands out.

A session opened by another session is listed by the directory it runs in,
like any other; the session it came from is told in its details
([details](details.md)). There is one tree: no view lists sessions by who
opened them.

Opening a terminal's row shows it where the key pressed says and moves the
keys there, as opening a session's row does ([content](content.md#opening)).
Closing a row's windows from the tree closes every window that shows it, and
leaves it running. Closing its
windows hides it; Stop Terminal
explicitly ends its program ([terminals](content.md#terminals)). Going from session to
session, the waiting sessions and the picker pass shells over.

`!` in the tree shows every worktree git knows of each workspace, those
where nothing runs too, drawn dim; opening one starts a session there.
Pressing it again goes back to the worktrees where sessions or shells run. The
window does not keep this between runs.

A session that has ended is listed no more; its agent keeps its
conversation, which can be resumed in a new session
([resuming](sessions.md#resuming-a-past-conversation)).

The filter at the top of the tree narrows it as it is typed into. The sessions
waiting on you are listed again at the bottom of the tree, apart from it.

The tree heads each [environment](environments.md)'s work with a row named
after it, which folds like a heading, one for every environment the window
shows, however many there are. The same directory in two environments is two
rows. Each heading says how its environment's connection stands: a mark alone
while it is connected, and in words while it is being reached, cannot be
reached, with how old its work is, reads only, or holds a change sent there
that needs you. A folded heading counts the sessions waiting on you inside it.
Selecting a heading shows its connection in the details: what went wrong, a
way to try again at once, and the drafts and changes waiting for it. The
sessions waiting on you are gathered from every environment, and going to the
next waiting session reaches each of them. A session, a shell or a Neovim
started from a row, or beside a tab, starts in the environment of that row or
tab.

## Going to a session or starting one

`Mod+p`, `w`, `+` beside the tree's filter, or the Session menu opens the
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

Choosing a session opens it as its row in the tree would, leaving the keys
where they were. Choosing a directory opens a new session there: the first
prompt sent starts it, named from its first words, a path among them by
its file name, or from a first line `name: <label>`. A prompt with no words
in ASCII letters, one written in Japanese for one, is named by its own first
letters, cut short.

A window showing more than one environment offers the sessions and the
directories of each, every row naming its environment, and a new session
starts in the environment its directory was chosen in. Its file tree and its
scratch row are of the environment of the selected row or the tab shown.

`Ctrl+f` turns the picker into a [file tree](#file-trees) for a directory the
list does not offer. It shows the directories under the home directory of the work machine it
browses, under each ghq root and under each folder the settings list, side by side, and
choosing one with the filer's open key starts a session there. `Ctrl+f` goes back to the list and
`Escape` closes the picker.

`n` on a row starts a session in that row's worktree or directory without the
picker. `Ctrl+q }` and `Ctrl+q {` open a session of the next and the
previous workspace without asking, the one last shown there.

## Scratch sessions

A **scratch session** is for talking to Claude about something no project
holds, with no directory to choose. The picker's first row or New
Scratch Session in the Session menu opens one, with the prompt ready as for a
new session, and so does a prompt sent with nothing open
([prompt](prompt.md)). The first prompt sent starts it in
a new directory of its own,
named by the local date and time, in the scratch folder: the one the settings
name, or `$XDG_DATA_HOME/adroit/scratch` (`~/.local/share/adroit/scratch`
when `$XDG_DATA_HOME` is unset). One never sent makes no directory.

The tree lists everything that runs in the scratch folder as one
workspace, Scratch, after every other, its sessions and shells straight under
it. Starting a session from it starts another scratch session in
a directory of its own; a shell opened from it runs in the scratch folder.

Adroit removes none of these directories. Claude Code keeps each conversation
by the directory it ran in, and the picker offers that directory among those
Claude Code has been used in. Its conversations are resumed from every
directory's past conversations
([resuming](sessions.md#resuming-a-past-conversation)).

Claude Code asks whether to trust a folder before a session first works in
it, and each scratch session works in a new one: its console shows the
question until it is answered there. Claude Code takes a folder outside git
as trusted when a folder above it is, so trusting the scratch folder itself,
by running `claude` once in a shell opened from Scratch, keeps the question
from coming back.

## File trees

The tree's files and the picker's directories are one file tree, drawn and
moved through alike, with the `filer` area's keys
([keys](reference/keys.md#actions)). Directories come first, each entry with
its Nerd Font icon in the colour vim-glyph-palette gives it, drawn from
Adroit's own Nerd Font whatever the console's font is. A directory is read
when it is opened. Going into a directory opens it and goes to its first
entry once it is read; going out closes the directory selected when it is
open, or else the one it is in, and goes there. Names with a leading dot are
listed only while the hidden are shown, and are drawn dim.

In the tree, going out of any row closes the one it sits under, and going
into a heading or a workspace goes to its first row, as in a directory.
