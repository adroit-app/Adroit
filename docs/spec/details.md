# The details panel

The details panel tells about what the content's focused window shows, and the
worktree it is in, on the sheets Overview, Changes, Log, Agents and Resources
([content](content.md)). It follows the focus from window to window and from
tab to tab, not the tree's selection; with nothing open, it has nothing to
describe. While an environment's heading is selected in the tree, it tells
instead how that environment's connection stands ([tree](tree.md)). A window
showing an image a session's console drew is described as that session, and
one showing a file in the worktree already described keeps the session
described before.

## Sheets

- **Overview** tells how the session the focused window shows stands now. At
  its head are the session's name, its state, the latest model, when it
  started and how long ago it last took input. Input sent through Adroit
  includes answers to approvals and choices; its latest time stays with the
  running session when the window closes. Below them is the change the
  worktree is on: its description's first line and how many files it
  changes; a click on it shows Changes.
  Under Running are what works for the session right now: the subagents
  still at work, its live work processes, and the sessions tied to it that
  still run. Under About, folded until it is unfolded, are the facts that
  change little: the tokens the whole conversation has used, its prompt and
  tool call counts, the profile and the login it started from, the workspace
  and the worktree, and the conversation's ID. Copying a fact copies its
  value. What was not reported is left out.
- **Changes** shows the current jj change’s full description, or the HEAD
  commit’s full message in Git, alongside the files the worktree changes.
  Opening a file shows its diff.
- **Log** lists the changes that lead to the worktree, drawn as a graph: each
  change is joined by lines to its parents, so forks and merges show. A line
  running off the end leads to a change not listed. Opening a change lists
  the files it changed, and opening one of those shows its diff in that
  change.
- **Agents** lists the other agents the session worked with. First the other
  sessions: the one that opened it and those it opened
  ([sessions that open sessions](opening-sessions.md)) while they run, then
  those it messaged or heard from through Adroit, ended ones included,
  newest first, each saying how the two are tied. Then the messages it sent
  and received through Adroit, newest first, each with who sent it to whom;
  opening one shows it whole. A message stays listed after the sessions on
  both ends have ended. Then the subagents the session set to work through
  Claude Code: those still at work, the latest first, then the few that ended
  last, each named by what it was last asked about, marked with what kind it
  is, and saying how it stands and for how long it has run or ran.
- **Resources** lists what the session's conversation brought up, newest
  first, for Claude Code, Codex, Pi and OpenCode alike: the claude.ai
  artifacts, the images, the other web addresses, and the files it wrote or
  edited, read, or named in a reply where such a file exists, each saying how
  the session came across it. An address inside a file it wrote, read or
  searched is the file's, not brought up. The images are those the session's
  console drew and the image files it came across that still exist,
  together, each beside a small picture of it.

A shell or a draft has no conversation, and Overview, Agents and Resources
show nothing for it.

A work process is one the session started; the agent and its supporting
runtimes are excluded from both the list and kill targets, while work
processes launched through those runtimes remain available. Processes that
cannot be associated with the session are omitted. Opening one shows its
command, process ID, parent, memory and working directory. Killing one from
its row or its details opens a confirmation dialog. Confirming verifies that
it is still the same process belonging to the session. A zombie has already
exited and must be reaped by its parent.

Opening a session of a sheet shows it, wherever it runs; one that runs no
more is listed by its name and says so. Copying one copies its name.

Each group shows its newest few, five unless the settings say otherwise
([settings](settings.md#settingstoml)), and a row that shows the rest;
opened, it shows them all until the group is folded or Adroit runs again.

A subagent is named as the session last summed up a message it sent it, or
by the description it was started with before any. Opening one shows what it
was started as, what it was asked, the latest steps it took, and, once it
ended, what it reported. Copying one copies what it was asked.

An image of Resources opens in the content as a file of the tree does: opened,
looked at in the preview, or opened in a split or a new tab, by the same
actions ([viewers](content.md#viewers)). An image file opens in the image
viewer; an image the console drew opens in the image viewer too, named for its
size. The keys stay in the panel, and the panel stays on the session while one
of its images is shown, so the next one can be picked. An image the console
drew is kept while Adroit runs, after the console has let it go, the newest
few of each session; it has no file to copy or reveal, and its window says
so once it is no longer kept.

Opening a link of Resources hands it to macOS: an address opens in the
default browser, a file in the app that opens it. A right click on a link
also offers to copy it, its address or its file's path, and on a file to
reveal it in the Finder. A click on a group's heading folds the group under it
or unfolds it, and a group stays folded as other sessions are shown. The list
follows the conversation as it goes on, and to the one `/clear` or `/resume`
moves the session to.

Back goes out one level at a time, to the list the shown row was opened from,
with that row selected. A row opens by its keys or a click; the line above a
row opened names what was opened, and a click on it goes back.

Usage follows the agent of the focused window on every details sheet, and the
account it is logged in as: that of the login the session runs in, or,
for a draft, that of the profile it starts from. It shows each reported
account limit as a bar of what is used, with what is left and
when it resets. A session's reported context utilization appears with those
limits. Shell and Neovim windows show no account limits or context utilization.

The usage limits are asked of the session's agent while the details panel is
in sight, again every few minutes, without starting a conversation or spending any of
them. An account without such limits, or an agent that does not answer,
shows none; one that stops answering keeps showing what it last said.
When the conversation does not record its context window, the context
utilization is asked of the agent the same way, again as the conversation
moves on, without changing or continuing the conversation.
A separate section below account usage shows live CPU, GPU and memory usage
for the whole machine and capacity used on the startup volume, including in
Shell and Neovim windows.
Each of these meters can be hidden independently in Settings. A meter that
cannot be read is marked unavailable. The meters have distinct colors; high
usage overrides those colors with a warning. Memory and storage include used
and total capacity; CPU and GPU include hardware information when available.

The Usage header links to the shown agent's account usage settings in the
default browser, where available resets and account limits can be managed.

The sheets follow what is read again while they are shown. A row opened that
is gone meanwhile, its file changed no more or its change rewritten out of
existence, is gone back from. Showing another worktree, or another sheet,
starts it from its list again.

## In a jj repository

A worktree inside a jj repository, a colocated one too, is read through jj:

- Changes lists what the working-copy change `@` changes.
- Log lists the changes from trunk up to `@`, `@` first, each with its short
  change ID, the first line of its description, and marks for `@`, an empty
  change, a conflict and its bookmarks. Without a trunk, it lists the
  ancestors of `@`, the most recent ones.

Reading a jj repository runs jj, which records the working copy into `@` as
any jj command does. It is read only while Overview, Changes or Log is shown. When jj
cannot be run, the repository is read through git instead.

## In a git repository

- Changes lists the files changed in the worktree against its last commit,
  staged or not, and each untracked file. An untracked file's diff shows all
  of it as added.
- Log lists the branch's most recent commits.

## Keys

In a list, `j` and `k` move, `gg` and `G` go to the first and the last row,
`Enter` or `l` opens the selected row, and `h`, `Escape` or `Backspace` go
back. In Overview, Agents and Resources, the same keys move over their headings
and rows and open a link, an image, a message, a process or a subagent; on a
heading, `Enter` or `l` folds or unfolds its group, the keys that go back fold
the group of what is selected, and `y` copies the selected row. In a row
opened, the keys that go back go back to its sheet. In a diff or a row
opened, `j`, `k`, `gg` and `G` scroll, and in every sheet `Ctrl+d` and
`Ctrl+u` scroll by half a page. `1` to `5` show the sheets ([keys](keys.md)).
