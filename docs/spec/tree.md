# The tree

Adroit lists every session it runs by the directory it runs in
([details](sessions.md)). What holds them is what git says about the directory
each runs in. Sessions started outside Adroit are not listed; their
conversations can be resumed in Adroit
([resuming](sessions.md#resuming-a-past-conversation)).

## Workspaces and worktrees

A **workspace** is where sessions and the shells you open in Adroit run:

- Scratch, holding every session and shell that runs in the scratch folder or
  below it, or in the temporary directory outside git
  ([scratch sessions](#scratch-sessions));
- a git repository, holding every session and shell that runs in any of its
  worktrees or in a directory below one;
- otherwise the directory a session or a shell runs in.

Inside a git workspace, each worktree where something runs has a row of its
own; a worktree where only shells or Neovim run is listed for them. A
workspace where everything runs in its main checkout shows that checkout's
contents straight under it, with no worktree row; one outside git always does.
A workspace where no session or terminal runs starts folded; once you fold or
unfold it, it stays as you left it.

A worktree's files sit under its Files heading, and what runs in it under
Sessions and Terminals: Terminals holds every program Adroit runs in a
terminal there, shells and Neovim alike, each marked by what it runs, save a
Neovim started on a file, which that file's row stands for. A terminal whose
program reports how it stands, as a session's does
([how a session stands](sessions.md#how-a-session-stands)), is marked with
that state as a session is, done until it is viewed; one whose program
reports nothing carries no such mark. A heading folds, and the row of one for
what runs counts what it holds. Where a workspace has no worktree row, the
headings are its checkout's.

A workspace with worktree rows lists, above them, its own Sessions and
Terminals: every session and terminal of all its worktrees, so what runs in
the repository is seen without opening each worktree. The worktree rows start
folded, each still holding its own Files, Sessions and Terminals. The row of
what a window shows is the one under the workspace's heading, or, while that
is folded, the one under its worktree's.

```text
api                        ← a git repository with sessions in two worktrees
├── Sessions               ← every session of every worktree of api
│   ├── auth-revamp
│   ├── review-api
│   └── implement
├── Terminals
│   └── $ dev-server
├── Pull Requests          ← what is open on GitHub, folded until opened
├── Issues
├── main                   ← a worktree, folded until opened
│   ├── Files
│   ├── Sessions
│   │   ├── auth-revamp
│   │   └── review-api
│   └── Terminals
│       └── $ dev-server
└── feat/oauth
    ├── Files
    ├── Sessions
    │   └── implement
    └── Terminals
notes                      ← a directory outside git: no worktree row
├── Files
└── Sessions
    └── outline-the-talk
Scratch                    ← the scratch folder and a temporary directory
├── Sessions
│   └── explain-regex-flags
├── Terminals
│   └── $ probe
├── scratch                ← the scratch folder
│   ├── Files
│   └── Sessions
│       └── explain-regex-flags
└── callback               ← a directory in a session's scratchpad
    ├── Files
    └── Terminals
        └── $ probe
```

A git workspace with a remote on GitHub lists Pull Requests and Issues
after the rows of what runs in it, while `gh` is installed on its work
machine; without `gh` it lists neither. Both start folded. Unfolded, each
lists what is open on GitHub, as the login
`gh` uses there sees it, each row by its title and who opened it, a pull
request marked with how its checks stand and drawn dim while it is a draft.
What is listed is read again every minute while the heading is open. Each
heading shows how many are open, all GitHub counts and not only those
listed, folded too: that count is read again every few minutes while the
heading is folded, and with its listing while it is open. When
`gh` cannot answer, because it is not logged in or GitHub cannot be reached,
the heading holds one row saying so.

Opening a row shows it in a tab of its own, as GitHub shows its
conversation: its title, how it stands, who opened it and, of a pull
request, the branches it merges; then a card for its body and for every
comment and review that says something, oldest first, each drawn as a
Markdown document is, with the images it shows, those of a private
repository too, and from a work machine too. A pull request's tab shows,
in turn, the files it changes with their diffs and the checks run on its
head, those that failed first. A link in it opens in the browser. The tab takes the viewer's keys, reads it again every minute while
it is shown and whenever the work is read again, and is not kept for the
next run. `g x` on its row or in its tab opens it on GitHub in the browser
instead, from a work machine too.

`n` on a row opens a new session on it, its first prompt already naming it
and left to finish and send: an issue's in the repository's checkout, a pull
request's in a new worktree fetched from GitHub: on the pull request's own
branch, tracking the remote's, when it comes from the repository, and at its
head, on a branch named after its number, when it comes from a fork.

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

A new file or directory is named in place, in the directory selected, the
one the file selected is in, or the worktree from its Files heading, and
made there on the work machine ([keys](reference/keys.md#actions)). A name
with `/` makes the directories it passes through too, and cannot reach
outside the directory it is typed in. What is there already is never
overwritten. What is made is selected once it is listed.

A file or a directory is copied or cut to the system clipboard, and pasted
where a new one would be made, as in fern
([keys](reference/keys.md#actions)). The clipboard carries its path: on this
computer as a file the Finder pastes too, and what the Finder copied pastes
into the tree; on a work machine elsewhere as text alone. What was cut moves
when it is pasted, after asking, as long as the clipboard still holds it;
anything else on the clipboard is copied. The work machine copies and moves.
Each name pasted that is taken already is asked about in turn, as in fern: it
is overwritten, the one there going to the work machine's trash first, pasted
by another name typed in place, or skipped. Trashing asks first, and puts it in
the work machine's trash. Files and directories marked, as in fern, are copied,
cut or trashed together in place of the one selected, and stand out until they
are.

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
([resuming](sessions.md#resuming-a-past-conversation)). A workspace stays
listed once anything has run in it, after everything there has ended and
across runs, until it is removed from the tree, from its row's menu or the
close button its row shows under the pointer. Where nothing runs, removing it
asks nothing; where something does, it asks first and then ends every session
and terminal running there. Starting anything in a removed workspace lists it
again. A workspace whose directory is gone is listed no more.

A worktree where something runs is closed the same ways: it asks first, then
ends every session and terminal running there, and its row goes while its
workspace stays listed.

The filter at the top of the tree narrows it as it is typed into, by the
fuzzy match the pickers use, fzf's syntax included, on the names of
sessions, terminals, worktrees and workspaces; a worktree or a workspace that
matches keeps all it holds, and the rows keep the tree's order. The sessions
waiting on you are listed again at the bottom of the tree, apart from it.

The tree heads each [environment](environments.md)'s work with a row named
after it, which folds like a heading, one for every environment the window
shows, however many there are. The same directory in two environments is two
rows. Each heading says how its environment's connection stands: a mark alone
while it is connected, and in words while it is being reached, cannot be
reached, with how old its work is, reads only, or holds a change sent there
that needs you. A folded heading counts the sessions waiting on you inside it.
The rows of every environment but the one of the selected row, or else of what
is shown, are drawn dimmer, all but the sessions waiting on you.
Selecting a heading shows its environment in the details: how its connection
stands, what went wrong, a way to try again at once or to pair with its work
machine again, the address and key of its work machine, its agents, the
drafts and changes waiting for it, and a way to its settings. The
sessions waiting on you are gathered from every environment, and going to the
next waiting session reaches each of them. A session, a shell or a Neovim
started from a row, or beside a tab, starts in the environment of that row or
tab. From a heading, a new session opens the picker on that environment, and a
shell or a Neovim starts in its host's home directory, once a host on a work
machine has said where that is. The `+` on a heading offers what starts
work in its environment: a new session, a shell, a Neovim, and Add to Tree
for any directory. A heading's menu also opens the environment's
settings, renames it in place, tries its work machine again at once while the
window waits to, pairs with its work machine again, and removes it.

## Going to a session or starting one

`Mod+p`, `w`, `+` beside the tree's filter, or the Session menu opens the
**picker**. Its first row starts a [scratch session](#scratch-sessions). It
lists every session next, each with where it runs and how it stands, and then
the directories to start a new session in:

- the directories the tree lists;
- the directories Claude Code has been used in, the most recent first;
- the repositories `ghq list` names, when ghq is installed and the settings
  do not turn it off;
- the folders in each folder the settings list for the picker
  ([settings](settings.md#settingstoml)).

A session stands as its row in the tree shows it, a finished one the user has
seen as idle. With nothing typed, the sessions are listed by how much they
need the user, those waiting on the user first and those finished unseen
next, and of those that stand alike the one the window last saw change comes
first. The tree keeps its own order.

Typing narrows both by a fuzzy match, fzf's syntax included: a session on its
name and where it runs, a directory on its path. The sessions stay above the
directories however well either answers, and the scratch row stays on top
while what is typed matches its words. A word `@` followed by a state, or the
start of one, keeps only the sessions in that state, `@w` those waiting on
the user, and offers nothing else. While the rows move as sessions change, the
cursor stays on the row it is on. The keys [over the window](keys.md#over-the-window)
move, choose and close the picker.

Choosing a session opens it as its row in the tree would, leaving the keys
where they were. Choosing a directory opens a new session there: the first
prompt sent starts it, named from a first line `name: <label>`, or else
from its first words, a path or a URL among them by what it names. A prompt
with words in another script, Japanese for one, is named by its own letters
as far as a row of the tree holds them, cut where a word likely ends. A name
made from the prompt stands until the session's agent
[names the session](sessions.md#renaming-a-session).

A window showing more than one environment offers the sessions and the
directories of each, every row naming its environment, and a new session
starts in the environment its directory was chosen in. Its file tree and its
scratch row are of the environment of the selected row or the tab shown.

`Ctrl+f` turns the picker into a [file tree](#file-trees) for a directory the
list does not offer. It shows the directories under the home directory of the work machine it
browses, under each ghq root and under each folder the settings list, side by side, and
choosing one with the filer's open key starts a session there. `Ctrl+f` goes back to the list, and
the filer's keys come before those over the window.

Instead of choosing a row, in the list or the file tree, `Ctrl+s` opens a
shell and `Ctrl+e` Neovim in its directory: a directory's own, the worktree a
session runs in, the scratch folder for the scratch row. `Ctrl+t` adds the
directory's workspace to the tree with nothing started there.

Typing a repository ghq can clone — `host/owner/repo` or a URL git clones
from — offers a row to clone it after
the directories, which typing a filter and `Enter` therefore never chooses
over a directory that matches, in each environment where ghq is offered as
above and does not keep that repository already. Choosing it runs `ghq get`
on that work machine in a terminal shown with the keys, so whatever the clone asks, a
passphrase or a host to trust, is answered there. Once the clone succeeds,
the repository's workspace is added to the tree, and its directory is taken
as if it had been chosen from the list,
by `Enter`, `Ctrl+s`, `Ctrl+e` or `Ctrl+t` as the clone row was; once it
fails, the terminal stays with what it wrote.

Typing `github:` looks through GitHub instead, and the list offers nothing
else. Where ghq is offered, a row says so while nothing, or the start of
`github`, is typed, and choosing it types `github:`. After `github:`, the list offers the owners
`gh` is logged in as on that work machine, the user and their
organizations, narrowed by what is typed; choosing one types it and a
slash. After `github:owner/`, it offers a clone row for each of that
owner's repositories that ghq does not keep, as `gh` lists them, the
private ones its login can see included, narrowed by what is typed after
the slash. Where `gh` is not installed or not logged in, no owner is
offered, and `github:owner/repo` typed in full offers that repository
alone.

Add to Tree, in the Session menu, on an environment's heading or `A` in the
tree, opens the picker on the directories alone, of the environment it was
opened in: choosing one, from the list or the file tree, adds its workspace
to the tree, and `Ctrl+s` and `Ctrl+e` still open a shell and Neovim there.

`n` on a row starts a session in that row's worktree or directory without the
picker. `Ctrl+q }` and `Ctrl+q {` open a session of the next and the
previous workspace without asking, the one last shown there.

## Scratch sessions

A **scratch session** is for talking to Claude about something no project
holds, with no directory to choose. The picker's first row or New
Scratch Session in the Session menu opens one, with the prompt ready as for a
new session, and so does a prompt sent with nothing open
([prompt](prompt.md)). Every scratch session runs in the scratch folder: the
one the settings name, or `$XDG_DATA_HOME/adroit/scratch`
(`~/.local/share/adroit/scratch` when `$XDG_DATA_HOME` is unset), made when
the first one starts.

The tree lists everything that runs in the scratch folder or below it as one
workspace, Scratch, after every other. Starting a session from it starts
another scratch session; a shell opened from it runs in the scratch folder.

Scratch also holds the throwaway directories sessions work in: each directory
in the temporary directory outside git where something runs, such as one in
the scratchpad Claude Code keeps for a session. Scratch lists each of them as
a repository lists a worktree, a row of its own named after the directory,
with the scratch folder as its main checkout; while only the scratch folder
has something running in it, its sessions and shells sit straight under
Scratch. A temporary directory's row goes once nothing runs there, and adding
one to the tree is refused. A git
repository in the temporary directory is a workspace of its own as any other.

A scratch session keeps to talk that belongs to no project. When the talk
turns to work on a project, it opens a session in that project's directory,
handing it what it needs, and asks that it be shown
([sessions that open sessions](opening-sessions.md)): while you are looking at
the scratch session, the new one opens in its place with the keys, and the
work carries on there.

Claude Code asks whether to trust a folder before a session first works in
it: the first scratch session shows the question in its console until it is
answered, and none asks again.

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
