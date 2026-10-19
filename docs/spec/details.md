# The details panel

The details panel tells about the worktree the console shows, on the sheets
Overview, Changes and Log.

## Sheets

- **Overview** names the session shown, how it stands, and the workspace and
  worktree it works in. Below, it lists what the session's conversation
  brought up, newest first: the claude.ai artifacts, the other web addresses,
  and the files it wrote or edited, read, or named in a reply where such a
  file exists, each saying how the session came across it. A shell or a
  draft has no conversation and lists none.
- **Changes** lists the files the worktree changes. Opening one shows its
  diff.
- **Log** lists the changes that lead to the worktree, drawn as a graph: each
  change is joined by lines to its parents, so forks and merges show. A line
  running off the end leads to a change not listed. Opening a change lists
  the files it changed, and opening one of those shows its diff in that
  change.

Opening a link of the overview hands it to macOS: an address opens in the
default browser, a file in the app that opens it. A right click on a link
also offers to copy it, its address or its file's path, and on a file to
reveal it in the Finder. A click on a group's heading folds the group under it
or unfolds it, and a group stays folded as other sessions are shown. The list
follows the conversation as it goes on.

Back goes out one level at a time, to the list the shown row was opened from,
with that row selected. A row opens by its keys or a click; the line above a
row opened names what was opened, and a click on it goes back.

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
back. In the overview, the same keys move over its headings and links and
open a link; on a heading, `Enter` or `l` folds or unfolds its group, the keys
that go back fold the group of what is selected, and `y` copies the selected
link. In a diff or an overview without links, `j`, `k`, `gg` and `G` scroll,
and in every sheet `Ctrl+d` and `Ctrl+u` scroll by half a page. `Cmd+1`,
`Cmd+2` and `Cmd+3` show the sheets ([keys](keys.md)).
