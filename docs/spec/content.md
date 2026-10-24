# Content

The middle of the window is the content. It holds tabs, each tab is split into
windows, and each window shows one thing: a session, a session not started
yet, a terminal adroit runs ([terminals](#terminals)), or a file in the viewer
of its kind ([viewers](#viewers)).

## Tabs

The tabs line up above the windows, numbered from 1. A tab is named after what
its focused window shows, with how many windows it holds, and keeps its own
focused window while another tab is shown. A tab holds whatever was opened in
it, from any worktree. Closing a tab closes every window in it.

Besides the tabs there may be one **preview**. It comes after every tab,
without a number, and is drawn apart from them, as is its window, so it is
not taken for a tab. It shows what was looked at last without being opened,
and the next thing looked at takes its place. It becomes a tab like any other
once what it shows is opened, once it is split, once keys are sent to what it
shows, or once the prompt takes the keys to write to it.

## Windows

A window is split side by side or one above the other; the new window lies
right of it or below it, shows the same thing and takes the focus. Splits
nest. Windows are resized by keys or by dragging the border between them,
moved to a side of their tab, exchanged, rotated and moved to a tab of their
own ([keys](reference/keys.md#actions)).

The same thing may stand in any number of windows and tabs. A program shown in
several windows runs at one size, the focused window's when it is one of them.

Each window has a strip on top that tells what it shows: its state, its name,
its agent, and the worktree it is in; a file's window shows the file's icon
and name, and a `×` at its right end closes the window. The focused window is
marked as the prompt is when it has the keys.
A session's window shows the agent's interface; the focused one shows the
prompt below it as well ([prompt](prompt.md)). A terminal's window takes every
key but the window commands' prefix and `Mod` keys. A viewer's window has no
prompt and takes the `viewer` area's keys.

## Opening

Nothing opened takes the place of what a window shows. Opening something
moves the focus to a window of any tab already showing it, and turns that tab
into an ordinary one when it is the preview; when no window shows it, it opens
in a new tab after the one shown.

Looking at something shows the tab already showing it, and closes the
preview, or else shows it in the preview. Going to the next or previous
session, workspace or waiting session looks at it.

From the tree, the key says how a row opens and the row says what: opened,
looked at with the keys staying in the tree, in a new window split below or beside the focused one, or in a new
tab though a window shows it already ([keys](reference/keys.md#actions)). A
row clicked is opened. A file opened from the tree opens by its kind, in a
viewer or in Neovim ([viewers](#viewers)).

## Closing

Closing a window or a tab only hides what it shows: sessions and terminals run
on, and the tree still reaches them. A viewer ends with the last window
showing it. A session not started yet that no window shows any longer is
given up.

A terminal whose program exits or a session that ends closes every window
that shows it. When that leaves nothing open, the next session or terminal of
its worktree is shown.

## Across runs

What the focused window showed when the window closed is shown again, in a tab
of its own, when a window with the same number opens
([windows](sessions.md#windows)); the other tabs and windows are not kept.

## Terminals

A terminal runs a shell, Neovim or any other program, apart from agent
conversations, and is listed beside the sessions of the worktree it runs in
([the tree](tree.md)). A shell is the account's login shell unless configured
otherwise; Neovim reads the user's own configuration. A command terminal runs
its program through the shell, as an interactive login shell would run it,
with the environment a shell gets, and is listed among the shells under the
name of its program unless given another. The Neovims of the prompt and the
console's reader are part of the window, not terminals.

Closing a terminal's windows hides it and leaves its program running; its row
in the tree opens it again, and closing the window leaves it running too. Stop
Terminal (`terminal.stop`) ends the selected or shown terminal, as its program
quitting does.

A terminal has an identity apart from its name and its directory. It is not an
agent session: it neither starts sessions nor sends or receives messages
between them.

### From the command line

These commands are run by the instance's work host, started when none runs,
and work without a window:

```sh
adroit terminal start --program shell --dir /src/api --json
adroit terminal start --program neovim --dir /src/api --json
adroit terminal start --program command --dir /src/api --json -- claude --debug
adroit terminal list --json
adroit terminal input --terminal <resource-id> --text "hello" --keys Tab Enter --json
adroit terminal screen --terminal <resource-id> --json
adroit terminal stop --terminal <resource-id> --json
```

`start` accepts an optional `--name`. Without `--json` it prints the new
resource ID. `list` prints resource IDs and names separated by a tab, one per
line; `screen` prints the visible screen without trailing empty rows. A
resource is addressed by ID, not by a name or the caller's directory.
`start` creates a terminal in the given directory, or the caller's current
directory when omitted. A command terminal takes its program and arguments
after `--`, and only it does.

`input` sends what it is given to the terminal's program, in the order given,
and returns once the terminal has taken it. `--text` is typed as it is, so a
newline in it is Enter; `--paste` is pasted, bracketed when the program asks
for it; `--keys` presses each key it names, written as the keys file writes a
key ([keys](keys.md#keys)): `Tab`, `Shift+Tab`, `Ctrl+g`, `Escape`,
`Enter`. A key reaches the program as it would from the window, in the
keyboard protocol the program asked for. A key that never reaches a program,
such as one held with Command, is refused, and nothing is sent. `screen`
reads the screen as it is and does not wait.

A JSON resource contains `resource_id` (UUID), `name`, `cwd`, `program`
(`shell`, `neovim` or `command`), and for a command terminal `command`, its
program and arguments. `list --json` returns an array of these resources.
`screen --json` returns `resource_id` and `lines`; `input --json` returns
`resource_id` and `sent`; `stop --json` returns `resource_id` and `stopped`.
Without `--json`, `input` and `stop` print the resource ID.

Invalid arguments exit with status 2. A missing resource, a failed launch or
an unreachable host exits with status 1.

## Viewers

A file that means nothing as text is shown by a viewer of its kind, drawn by
the window itself: images, movies, HTML pages and PDFs. Every other file,
including one of a kind adroit does not know, opens in Neovim.

A file is opened or looked at from the tree as any row is: in its viewer, or in a Neovim started on it in its worktree, which the file's row stands
for in the tree. A file already open in a Neovim started on it opens that Neovim
again. A viewer ends with the windows showing it; it keeps its zoom, page and
playback while the focus moves between windows.

A file's kind is decided by its extension alone, whatever its case. The common
image formats open in the image viewer, the common movie formats in the movie
viewer, `.html` and `.htm` in the HTML viewer, and `.pdf` in the PDF viewer.

- **Image** draws the picture whole within the window, and zooms in and out
  from there; zooming back to the fit shows it whole again.
- **Movie** plays and pauses the movie and moves back and forward a few
  seconds at a time. The time played and the movie's length are shown.
- **HTML** draws the page as a browser would, scripts included. It shows a
  file, never an address.
- **PDF** shows one page at a time, turned by keys, and zooms like an image.
  The page shown and the page count are shown.

In every viewer the content scrolls by lines, by half the window, and to its
top and bottom, and the file's path can be copied.

A viewer only reads its file, and reads nothing else, with one exception: an
HTML page also reads the files under its own directory, which it links to by
relative paths. A page cannot reach files outside that directory, nor the
window it is drawn in, nor adroit.

A viewer's keys are actions of the `viewer` area, bound in the keys file like
any other ([actions](reference/keys.md#actions)). They reach the viewer of the
focused window alone. Keys pressed while an HTML page has the focus reach the
viewer as well, except those typed into the page's own fields.

## Images in the console

Programs in a session or a terminal can show images with the
[kitty graphics protocol](https://sw.kovidgoyal.net/kitty/graphics-protocol/),
so tools that draw plots or previews in a terminal draw them in adroit too.
Claude Code draws its own pictures, such as diagrams a mod renders, in the
sessions adroit starts: they are told the console draws images, which Claude
Code otherwise believes only of kitty and Ghostty.

The protocol is understood as felis understands it, which follows kitty:
images sent in the escape sequence, in a file or in shared memory, at once or
in chunks, as PNG or raw pixels, compressed or not; put as they are sent or
later by id or number, whole or a part of them; put by placeholder
characters; moved with the lines they are on and taken away as kitty takes
them away. A program asking whether images work is answered.

Images are drawn over the text, the later ones above the earlier, and an
animation shows the frame the program last showed.

The images a session's console drew are listed in the overview of the
details panel, and stay listed after the console lets them go
([details](details.md)).
