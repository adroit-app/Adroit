# The details panel

The details panel tells about what the content's focused window shows, and the
worktree it is in, on the sheets Overview, Changes and Log
([content](content.md)). It follows the focus from window to window and from
tab to tab, not the tree's selection; with nothing open, it has nothing to
describe.

## Sheets

- **Overview** names the session the focused window shows, how it stands, and the workspace and
  worktree it works in. Conversation facts include the latest model, elapsed
  time since the last user input, the tokens the whole conversation has used,
  and its prompt and tool call counts.
  Input sent through Adroit includes answers to approvals and choices; its
  latest time stays with the running session when the window closes. For
  sessions in other terminals, only inputs recorded in the conversation can
  be recovered. Unreported metrics are omitted. The Processes group lists live
  work processes belonging to the session. The agent and its supporting
  runtimes are excluded from both the list and kill targets; work processes
  launched through those runtimes remain available. Opening one
  shows its command, process ID, parent, memory and working directory. Killing
  one from its row or its details opens a confirmation dialog. Confirming
  verifies that it is still the same process belonging to the session. A zombie has already exited and must be reaped by
  its parent. Processes that cannot be associated with the session are omitted.
  Below, it lists the other sessions it worked with:
  the one that started it and those it started
  ([sessions that start sessions](delegation.md)) while they run, then those
  it messaged or heard from through Adroit, ended ones included, newest
  first, each saying how the two are tied. Then it lists the messages it sent
  and received through Adroit, newest first, each with who sent it to whom;
  opening one shows it whole. A report stays listed after the sessions on
  both ends have ended. Then it lists the subagents the session set to work
  through Claude Code: those still at work, the latest first, then the few
  that ended last, each named by what it was last asked about, marked with
  what kind it is, and saying how it stands and for how long it has run or
  ran. After them it lists what the session's conversation brought up,
  newest first, for both Claude Code and Codex: the claude.ai artifacts, the
  images, the other web addresses, and the files it wrote or edited, read, or
  named in a reply where such a file exists, each saying how the session came
  across it. An address inside
  a file it wrote, read or searched is the file's, not brought up. The images
  are those the session's console drew and the image files it came across
  that still exist, together, each beside a small picture of it. A shell or a
  draft has no conversation and lists none.
- **Changes** shows the current jj change’s full description, or the HEAD
  commit’s full message in Git, alongside the files the worktree changes.
  Opening a file shows its diff.
- **Log** lists the changes that lead to the worktree, drawn as a graph: each
  change is joined by lines to its parents, so forks and merges show. A line
  running off the end leads to a change not listed. Opening a change lists
  the files it changed, and opening one of those shows its diff in that
  change.

Opening a session of the overview shows it, wherever it runs; one that runs
no more is listed by its name and says so. Copying one copies its name.

Each kind of link shows its newest few, five unless the settings say
otherwise ([settings](settings.md#settingstoml)), and a row that shows the
rest; opened, it shows them all until the group is folded or adroit runs
again.

A subagent is named as the session last summed up a message it sent it, or
by the description it was started with before any. Opening one shows what it
was started as, what it was asked, the latest steps it took, and, once it
ended, what it reported. Copying one copies what it was asked.

Opening an image of the overview shows it whole in a window of its own,
named for it, the image fit within the window; opening it again brings that
window forward. `Escape` or `Cmd+W` closes it. An image the console drew is
kept while adroit runs, after the console has let it go, the newest few of
each session; it has no file to copy or reveal.

Opening a link of the overview hands it to macOS: an address opens in the
default browser, a file in the app that opens it. A right click on a link
also offers to copy it, its address or its file's path, and on a file to
reveal it in the Finder. A click on a group's heading folds the group under it
or unfolds it, and a group stays folded as other sessions are shown. The list
follows the conversation as it goes on, and to the one `/clear` or `/resume`
moves the session to.

Back goes out one level at a time, to the list the shown row was opened from,
with that row selected. A row opens by its keys or a click; the line above a
row opened names what was opened, and a click on it goes back.

Usage follows the agent of the focused window on every details sheet. It shows
each reported account limit as a bar of what is used, with what is left and
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
Each resource can be hidden independently in Settings. A resource that cannot
be read is marked unavailable. Resources have distinct colors; high usage
overrides those colors with a warning. Memory and storage include used and
total capacity; CPU and GPU include hardware information when available.

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
open a link, an image or a subagent; on a heading, `Enter` or `l` folds or unfolds its
group, the keys that go back fold the group of what is selected, and `y`
copies the selected row. In a subagent opened, the keys that go back go back
to the overview. In a diff, a subagent opened or an overview without links,
`j`, `k`, `gg` and `G` scroll, and in every sheet `Ctrl+d` and `Ctrl+u` scroll by half a page. `Mod+1`,
`Mod+2` and `Mod+3` show the sheets ([keys](keys.md)).
