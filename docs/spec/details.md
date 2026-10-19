# The details panel

The details panel tells about the worktree the console shows, on the sheets
Overview, Changes and Log.

## Sheets

- **Overview** names the session shown, how it stands, and the workspace and
  worktree it works in. Below, it lists the other sessions it worked with:
  the one it was started from and those it started
  ([sessions](sessions.md#started-by-a-session)) while they run, then those it
  messaged or heard from through Claude Code, newest first, each saying how
  the two are tied. Then it lists the subagents the session set to work
  through Claude Code: those still at work, the latest first, then the few
  that ended last, each named by what it was last asked about, marked with
  what kind it is, and saying how it stands and for how long it has run or
  ran. After them it lists what the session's conversation brought up,
  newest first: the claude.ai artifacts, the other web addresses,
  and the files it wrote or edited, read, or named in a reply where such a
  file exists, each saying how the session came across it. An address inside
  a file it wrote, read or searched is the file's, not brought up. A shell or a
  draft has no conversation and lists none. At its foot, whatever is shown
  above, it tells how much of the account's usage limits is left: each limit
  Claude Code's `/usage` reports, such as the current session and the week,
  as a bar of what is used, with what is left and when it resets.
- **Changes** lists the files the worktree changes. Opening one shows its
  diff.
- **Log** lists the changes that lead to the worktree, drawn as a graph: each
  change is joined by lines to its parents, so forks and merges show. A line
  running off the end leads to a change not listed. Opening a change lists
  the files it changed, and opening one of those shows its diff in that
  change.

Opening a session of the overview shows it, wherever it runs; one that runs
no more is listed by its name and says so. Copying one copies its name, which
Claude Code's messaging addresses it by.

Each kind of link shows its newest few, five unless the settings say
otherwise ([settings](settings.md#settingstoml)), and a row that shows the
rest; opened, it shows them all until the group is folded or adroit runs
again.

A subagent is named as the session last summed up a message it sent it, or
by the description it was started with before any. Opening one shows what it
was started as, what it was asked, the latest steps it took, and, once it
ended, what it reported. Copying one copies what it was asked.

Opening a link of the overview hands it to macOS: an address opens in the
default browser, a file in the app that opens it. A right click on a link
also offers to copy it, its address or its file's path, and on a file to
reveal it in the Finder. A click on a group's heading folds the group under it
or unfolds it, and a group stays folded as other sessions are shown. The list
follows the conversation as it goes on.

Back goes out one level at a time, to the list the shown row was opened from,
with that row selected. A row opens by its keys or a click; the line above a
row opened names what was opened, and a click on it goes back.

The usage limits are asked of Claude Code while the overview is in sight,
again every few minutes, without starting a conversation or spending any of
them. An account without such limits, or a Claude Code that does not answer,
shows none; one that stops answering keeps showing what it last said.

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
any jj command does. It is read only while Changes or Log is shown. When jj
cannot be run, the repository is read through git instead.

## In a git repository

- Changes lists the files changed in the worktree against its last commit,
  staged or not, and each untracked file. An untracked file's diff shows all
  of it as added.
- Log lists the branch's most recent commits.

## Keys

In a list, `j` and `k` move, `gg` and `G` go to the first and the last row,
`Enter` or `l` opens the selected row, and `h`, `Escape` or `Backspace` go
back. In the overview, the same keys move over its headings and rows and
open a link or a subagent; on a heading, `Enter` or `l` folds or unfolds its
group, the keys that go back fold the group of what is selected, and `y`
copies the selected row. In a subagent opened, the keys that go back go back
to the overview. In a diff, a subagent opened or an overview without links,
`j`, `k`, `gg` and `G` scroll, and in every sheet `Ctrl+d` and `Ctrl+u` scroll by half a page. `Cmd+1`,
`Cmd+2` and `Cmd+3` show the sheets ([keys](keys.md)).
