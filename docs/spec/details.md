# The details panel

The details panel tells about what the content's focused window shows, and the
worktree it is in ([content](content.md)). It follows the focus from window to
window and from tab to tab, not the tree's selection; with nothing open, it has
nothing to describe. While an environment's heading is selected in the tree, it
tells instead of that environment, and while a pull request's or an issue's
row is selected, of that ([tree](tree.md)). A window showing an image a
session's console drew is described as that session, and one showing a file
in the worktree already described keeps the session described before.

The panel is made of [components](#components), each one piece of what it can
tell. They are arranged into tabs, and into a foot shown under every tab, once
for each thing the panel describes: what a window shows, an environment, and a
pull request or issue. Adroit comes with an arrangement of its own, and you
can make your own ([below](#arranging-the-panel)). Each of the three comes
back to the tab it was last left on. Clicking a tab shows it; `1` to `9`, in
the panel, show the tab of that number ([keys](keys.md)). With a single tab,
no tabs are drawn.

As Adroit comes, a window's panel has the tabs Overview (the session, its
change, a summary of its timeline, what runs for it and, folded, About),
Changes, Log, Agents (sessions, messages and subagents), Resources
(artifacts, images, links and files) and Timeline, over a foot of the agent's
usage and the machine's; an environment's has its connection and its
profiles over the machine's usage; and a pull request's or issue's has it
alone.

## Components

What only a window shows can be placed only where a window is described; what
is of an environment, anywhere, where it is of the environment of the row
described or else of the one shown; and a pull request or issue where a
window or such a row is described.

- **session** — the session's name, its state, the latest model, when it
  started and how long ago it last took input. A session waiting on the user
  or failed shows, beside its state, what its program said of why. Input
  sent through Adroit includes answers to approvals and choices; its latest
  time stays with the running session when the window closes.
- **change** — the change the worktree is on: its description's first line
  and how many files it changes; a click on it shows the tab that holds
  **changes**.
- **progress** — once the session's agent has recorded a timeline, a summary
  of it: the tree at work, its status, how much of it is completed, the task
  in progress in it, or else the next one to do, how many of its leaf tasks
  are completed and how long it has taken. Opening it shows the tab that
  holds **timeline**, with that task selected.
- **timeline** — the task trees the session's agent recorded of its work
  ([below](#the-timeline)).
- **running** — what works for the session right now: the subagents still at
  work, its live work processes, and the sessions tied to it that still run.
- **about** — the facts that change little: the tokens the whole
  conversation has used, its prompt and tool call counts, the profile and the
  login it started from, the workspace and the worktree, and the
  conversation's ID. Copying a fact copies its value. What was not reported
  is left out.
- **sessions** — the other sessions the session worked with: the one that
  opened it and those it opened
  ([sessions that open sessions](opening-sessions.md)) while they run, then
  those it messaged or heard from through Adroit, ended ones included,
  newest first, each saying how the two are tied.
- **messages** — the messages it sent and received through Adroit, newest
  first, each with who sent it to whom; opening one shows it whole. A message
  stays listed after the sessions on both ends have ended.
- **subagents** — the subagents the session set to work through Claude Code:
  those still at work, the latest first, then the few that ended last, each
  named by what it was last asked about, marked with what kind it is, and
  saying how it stands and for how long it has run or ran.
- **artifacts**, **images**, **links** and **files** — what the session's
  conversation brought up, newest first, for Claude Code, Codex, Pi and
  OpenCode alike: the claude.ai artifacts, the images, the other web
  addresses, and the files it wrote or edited, read, or named in a reply
  where such a file exists, each saying how the session came across it. An
  address inside a file it wrote, read or searched is the file's, not
  brought up. The images are those the session's console drew and the image
  files it came across that still exist, together, each beside a small
  picture of it.
- **changes** — the current jj change's full description, or the HEAD
  commit's full message in Git, alongside the files the worktree changes.
  Opening a file shows its diff.
- **log** — the changes that lead to the worktree, drawn as a graph: each
  change is joined by lines to its parents, so forks and merges show. A line
  running off the end leads to a change not listed. Opening a change lists
  the files it changed, and opening one of those shows its diff in that
  change.
- **agent-usage** — the usage of the agent of the focused window
  ([below](#usage)).
- **machine** — the machine's CPU, GPU, memory and storage
  ([below](#usage)).
- **environment** — how the environment's connection stands, when it last
  worked, the address and key of its work machine, what went wrong, the
  changes and drafts waiting for it, and buttons to try again, pair again
  and open Settings on it.
- **profiles** — each agent profile of the environment, with its agent,
  whether it is the default, whether its login is logged in, as that
  environment's machine says, and, for an agent whose account has usage
  limits, what is left of them for that login with a link to the agent's
  usage page.
- **github** — the pull request or issue selected, or, of what a window shows,
  the pull request open for the branch its worktree has checked out, as last
  read: its title,
  where it is on GitHub, who opened it and when it was last updated, its
  labels, assignees and milestone, and of a pull request its branches, how
  much it changes, how its review stands and who it awaits, and how many of
  its checks passed, failed or still run, naming those that failed. Buttons
  show it, open it on GitHub and start a session on it, as its row's keys do
  ([tree](tree.md)).

A shell or a draft has no conversation, and what is read of one shows
nothing for it. A component that lists rows lists them under a heading of
its own, which folds and unfolds them; one alone in its tab lists them
without a heading.

## Arranging the panel

The arrangement is this computer's, for every environment, kept with its
other display preferences ([settings](settings.md)) and written in
`settings.toml` as the table `[panel]`
([reference](reference/settings.md#panel)). It holds `session`, `environment`
and `github`, each what the panel shows of that subject; one left out is as
Adroit has it, and one given replaces Adroit's whole.

```toml
[panel.session]
foot = ["agent-usage", "machine"]

[[panel.session.tabs]]
title = "Work"
icon = "info-circle"
components = ["session", "change", "running", "files"]

[[panel.session.tabs]]
title = "Code"
components = [
  "changes",
  { component = "log", folded = true },
  { component = "machine", meters = { gpu = false } },
]
```

- `tabs` lists the tabs in order, at least one. Each has a `title`, an
  optional `icon` naming one of the icons Adroit draws by its name in Tabler,
  and its `components`, top first.
- `foot` lists the components shown under every tab; none when left out.
- A component is written by its name, or as a table with its name in
  `component` and how it shows there: `folded` starts its heading folded or
  unfolded, `shown` is how many rows it shows before a row that shows them
  all, and `meters`, of **machine**, which of `cpu`, `gpu`, `memory` and
  `storage` it shows, each shown unless set to `false`. Unset, a component
  shows as Adroit has it, and **machine** the meters the Settings window
  chooses.

A component placed where it cannot tell anything, an icon Adroit does not
have, or a subject with no tab, is refused as any setting out of form is.
A work process is one the session started; the agent and its supporting
runtimes are excluded from both the list and kill targets, while work
processes launched through those runtimes remain available. Processes that
cannot be associated with the session are omitted. Opening one shows its
command, process ID, parent, memory and working directory. Killing one from
its row or its details opens a confirmation dialog. Confirming verifies that
it is still the same process belonging to the session. A zombie has already
exited and must be reaped by its parent.

Opening a session a component lists shows it, wherever it runs; one that runs no
more is listed by its name and says so. Copying one copies its name.

Each group shows its newest few, five unless the settings say otherwise
([settings](settings.md#settingstoml)), and a row that shows the rest;
opened, it shows them all until the group is folded or Adroit runs again.

A subagent is named as the session last summed up a message it sent it, or
by the description it was started with before any. Opening one shows what it
was started as, what it was asked, the latest steps it took, and, once it
ended, what it reported. Copying one copies what it was asked.

An image of **images**, opened or clicked, is looked at in the lightbox among
the others ([images in the console](content.md#images-in-the-console)).
Opened in a split or a new tab, by the same actions as a file of the tree
([viewers](content.md#viewers)), an image file opens in the image viewer, and
an image the console drew does too, named for its size. The keys stay in the panel, and the panel stays on the session while one
of its images is shown, so the next one can be picked. An image the console
drew is kept while Adroit runs, after the console has let it go, the newest
few of each session; it has no file to copy or reveal, and its window says
so once it is no longer kept.

Opening a link of **artifacts**, **links** or **files** hands it to macOS: an address opens in the
default browser, a file in the app that opens it. A right click on a link
also offers to copy it, its address or its file's path, and on a file to
reveal it in the Finder. A click on a group's heading folds the group under it
or unfolds it, and a group stays folded as other sessions are shown. The list
follows the conversation as it goes on, and to the one `/clear` or `/resume`
moves the session to.

Back goes out one level at a time, to the list the shown row was opened from,
with that row selected. A row opens by its keys or a click; the line above a
row opened names what was opened, and a click on it goes back.

## Usage

**agent-usage** follows the agent of the focused window, and the
account it is logged in as: that of the login the session runs in, or,
for a draft, that of the profile it starts from. It shows each reported
account limit as a bar of what is used, with what is left and
when it resets. A session's reported context utilization appears with those
limits. Shell and Neovim windows show no account limits or context utilization.

The usage limits are asked of the session's agent while **agent-usage** is
in sight, again every few minutes, without starting a conversation or spending any of
them. Those of an environment's profiles are asked the same way, on that
environment's machine, only while **profiles** is in sight. An account without such limits, or an agent that does not answer,
shows none; one that stops answering keeps showing what it last said.
When the conversation does not record its context window, the context
utilization is asked of the agent the same way, again as the conversation
moves on, without changing or continuing the conversation.
**machine** shows live CPU, GPU and memory usage for the whole machine and
capacity used on the startup volume, in Shell and Neovim windows too. The
machine is that of the environment of the row selected in the tree, or else
of what is shown. Which meters it shows is chosen in Settings, once for every
environment, or by the component where it is placed, and a work machine
samples only the meters some window shows of it. A meter that
cannot be read is marked unavailable. The meters have distinct colors; high
usage overrides those colors with a warning. Memory and storage include used
and total capacity; CPU and GPU include hardware information when available.

The heading of **agent-usage** links to the shown agent's account usage
settings in the default browser, where available resets and account limits can be managed.

## Following what is read

The components follow what is read again while they are in sight; one in no
tab, or in a tab not shown, is not read. A row opened that
is gone meanwhile, its file changed no more or its change rewritten out of
existence, is gone back from. Showing another worktree, or another tab,
starts it from its list again.

## The timeline

A session's agent records the plan of a goal that takes several steps as a
tree of tasks, through Adroit's own tools
([sessions that open sessions](opening-sessions.md#what-a-session-can-do)),
and marks each task as it starts, finishes or is blocked. It is the same for
Claude Code, Codex, Pi and OpenCode, is kept for as long as the session's
record is, and goes on with the session when it is brought back. A session
whose agent never recorded one has an empty Timeline.

Each tree is a card: its title, its status, a bar split into what is
completed, in progress, blocked and not started, the share completed, how
many of its leaf tasks are completed and how long it has taken, until it was
completed or until now. Under the card are its tasks, each step under the
task it is a step of, each with its status, a bar and its share. The tree at
work comes first, then the newest. A completed tree shows its card alone
until it is unfolded, any other its tasks until it is folded, and a card
opened folds or unfolds it. Opening a task shows what it is for, what came of
it, its share, its weight and its leaf tasks completed, and its history,
newest first: each change with how long ago it was made, the status after it,
the task's share before and after it, and the description and note it set.
The timeline follows what the agent records while it is shown.

Siblings split their parent's share of the work by their weights, so
splitting a task into steps leaves its share as it was, and a completed task
counts whole whatever its steps say. A tree is completed once every leaf task
is, or its top task is; blocked while a leaf task is blocked; in progress once
a leaf task is in progress or completed; and not started otherwise.

## In a jj repository

A worktree inside a jj repository, a colocated one too, is read through jj:

- Changes lists what the working-copy change `@` changes.
- Log lists the changes from trunk up to `@`, `@` first, each with its short
  change ID, the first line of its description, and marks for `@`, an empty
  change, a conflict and its bookmarks. Without a trunk, it lists the
  ancestors of `@`, the most recent ones.

Reading a jj repository runs jj, which records the working copy into `@` as
any jj command does. It is read only while **change**, **changes** or **log** is in sight, or a
change or file opened from them is shown. When jj
cannot be run, the repository is read through git instead.

## In a git repository

- Changes lists the files changed in the worktree against its last commit,
  staged or not, and each untracked file. An untracked file's diff shows all
  of it as added.
- Log lists the branch's most recent commits.

## Keys

In a list, `j` and `k` move, `gg` and `G` go to the first and the last row,
`Enter` or `l` opens the selected row, and `h`, `Escape` or `Backspace` go
back. Over the rows of the components in sight, the same keys move over
their headings and rows and open a link, an image, a message, a process, a
subagent, a file changed or a change of the log; on a
heading, `Enter` or `l` folds or unfolds its group, the keys that go back fold
the group of what is selected, and `y` copies the selected row. In a row
opened, the keys that go back go back to its tab. In Timeline, a tree's card
folds and unfolds as a heading does, and the keys that go back from a task
fold its tree. In a diff or a row
opened, `j`, `k`, `gg` and `G` scroll, and in every tab `Ctrl+d` and
`Ctrl+u` scroll by half a page.
