# Keys

Every key adroit handles itself runs a named action. adroit ships bindings of
its own, and a file you write changes, adds to or takes away from them. The
prompt's keys are Neovim's and are set up in Neovim; the dialog that asks
before taking over a session always answers to `y`, `Enter`, `n` and `Escape`,
and the picker's keys are its own ([workspaces](workspaces.md#going-to-a-session-or-starting-one)).

## In the menu bar

The menu bar offers every action that is more than a key, beside macOS's own
entries for the application, editing and windows. An entry runs its action
where the keys are, as its key would, but not while the picker, a dialog or a
name being typed holds the keys; from the Settings window, it runs in the
main window and brings it forward, but for closing a tab or the window, which
closes the Settings window.

Each entry shows the Command key bound to its action in `global`, when that
key runs it in every area. An action bound only to a sequence, to keys
without Command, or to a Command key some area binds otherwise shows none,
and so does a key the menu cannot show as it is typed, such as `Cmd+}`.
The entries follow the file as it changes.

## With the mouse

What the keys do to a sidebar row or a tab of the console is also offered by
right-clicking it, each entry with the keys bound to it. A tab closes with its
`×`, a workspace's or worktree's row starts a session there with its `+`, and
the buttons beside Workspaces filter the sidebar, list every worktree, read
the sessions again and start a session.

A click on the console, live or read, reaches a program that reads the mouse,
as Claude Code does, at the cell clicked; a program that does not read it is
told nothing.

## Where the keys go

Keys go where the mode chip at the bottom left says, whatever in the window a
click or macOS left focused, but for a text field being typed in that is
shown: the filter, a name being typed, the picker's query. When the window is
in front and its keys have strayed elsewhere, it takes them back by itself
within a second, without a click.

With `$ADROIT_DEBUG_KEYS` set to anything but empty, the window writes to its
standard error, line by line, each key it receives, where focus moves and
where each key goes.

## Where the file is

- `$ADROIT_KEYS`, when set;
- otherwise `keys.toml` in `$XDG_CONFIG_HOME/adroit/`, or in
  `~/.config/adroit/` when `$XDG_CONFIG_HOME` is unset.

A missing file keeps adroit's own bindings. A binding to `coordinator.open`,
`project.new`, `project.edit`, `workspace.pick`, `details.files` or
`details.diff`, actions earlier versions had, is read and dropped. The window
reads the file again when it changes; a mistake in it is reported at the bottom
of the window, and the bindings in force stay as they were.

## Changing them in the Settings window

The Settings window lists the bindings in force, area by area, marking those
the file changed, added or took away. A binding is changed, added or deleted
there, its keys typed or recorded as they are pressed, and keys it would
collide with are pointed out. One put back, or every one at once, is the
default again. Each change is written into the file at once, its comments and
the rest of it left as they were; a file with a mistake in it is not changed
there until it is fixed.

## Format

```toml
[sidebar]
"Ctrl+n" = "cursor.down"
"Ctrl+p" = "cursor.up"
"t" = "none"
"T" = "session.take_over"

[console]
"Ctrl+c" = "console.send Ctrl+c"
```

Each table is an area of the window, and each entry binds a sequence of keys
to an action. A binding for keys an area already binds replaces it; `"none"`
takes the keys away, so they do nothing there.

### Areas

| Area | Holds the keys for |
| --- | --- |
| `global` | every area, after the area's own |
| `sidebar` | the sidebar, and the details panel after its own |
| `details` | the details panel |
| `console` | the console while it shows a session |
| `prompt` | the prompt, before Neovim |

A key that reaches a program in the console is sent as the program asked,
as felis sends it: with the Kitty keyboard protocol once it asks for it, as
xterm sends it otherwise.
A shell in the console takes every key that is not a `global` binding, and
shows no prompt below it: a window move does not stop there, and the `prompt`
actions do nothing. A
session waiting on a permission prompt or a question does too, as its answer,
from the prompt as well as the console; the prompt is Neovim's again once it
is answered. So does a session showing a dialog of Claude Code's own in place
of its input box, or above it while Claude works, such as the picker `/model`
opens, until the dialog closes.
The dialog is told from what the console shows; one adroit does not
recognise still takes `console.pass_every_key`.
The mouse wheel scrolls the console back in either case, and what is sent to
it shows the bottom again.
A key the `console` area does not bind, in a session's console, starts
reading it: what the console holds, scrolled off and on screen, colours kept,
is read in Neovim, with your own Vim keys for moving, scrolling, searching and
selecting, and a yank copies to the clipboard. It follows what the session
writes meanwhile. `q` or `Esc` in normal mode goes back to the console live,
and `i`, `a` and `o` to the prompt; `Ctrl-w` sequences stay `global`, and
one that ends no binding does nothing. The reader is for reading only: `:`,
Ex mode, quitting and Vim's window commands do nothing in it, and a window or
a tab a mapping or a plugin opens in it closes at once.
A program that scrolls what it shows itself, as Claude Code does on the
alternate screen, leaves nothing scrolled off to read: moving or scrolling past the reader's
first or last line scrolls the program instead, and the reader follows; `G`
and `gg` scroll it to its end and its start. Leaving the reader by these keys
scrolls it back to its end.
In the prompt, `global` bindings apply only outside Neovim's insert mode.

### Keys

A key is written as its modifiers, then its name, joined by `+`: `Ctrl+w`,
`Shift+Tab`, `Ctrl+Alt+x`, `Cmd+p`. The modifiers are `Cmd`, `Ctrl`, `Alt`
and `Shift`. Command with `c`, `v`, `x`, `a`, `z`, `q`, `h` and `m` stays
with the menu: editing, quitting, hiding and minimizing.
A Command key reaches the `global` bindings wherever the keys are, the
prompt's insert mode and a console passing its keys on included.
A key is a single character, `Space`, `Plus`, `F1` to `F12`, or one of
`Enter`, `Escape`, `Tab`, `Backspace`, `Delete`, `Insert`, `Home`, `End`,
`PageUp`, `PageDown`, `ArrowUp`, `ArrowDown`, `ArrowLeft` and `ArrowRight`. A
character carries its own case, so `G` is Shift and g, and `Shift+g` means the
same. Any other character is written as the one Shift types, `!` rather than
`Shift+1`.

A sequence is keys separated by spaces, pressed one after another: `g g`,
`Ctrl+w h`. When the keys pressed so far complete one binding and start
another, the complete one runs.

### Actions

| Action | Does |
| --- | --- |
| `window.left`, `window.down`, `window.up`, `window.right` | move to the area in that direction |
| `tab.next`, `tab.prev` | show the next or previous tab of the area the keys are in: the details panel's sheets there, elsewhere the console's tabs, which are the sessions opened in the window, the worktree's shells and a draft |
| `tab.close` | close the current tab; a shell ends, a session runs on and stays in the sidebar. On a shell's row in the sidebar, that shell ends |
| `window.close` | close the window, which ends adroit; its sessions run on |
| `cursor.down`, `cursor.up`, `cursor.first`, `cursor.last` | move the selection in the sidebar; in the details panel, in the list it shows, or scroll what else it shows |
| `cursor.open` | open the selected session, shell or worktree and move to its console, or to the prompt of a session not started yet; fold or unfold a workspace; in the details panel, open the selected row |
| `cursor.show` | show the selected session, shell or worktree in the console, as `cursor.open` does, with the keys staying in the sidebar; fold or unfold a workspace; in the details panel, open the selected row |
| `cursor.fold` | fold the selected node; in the details panel, go back out to the list what it shows was opened from |
| `cursor.copy` | in the details panel, copy the selected link of the overview: its address, or its file's path |
| `cursor.reveal` | in the details panel, show the selected file of the overview in the Finder |
| `waiting.next`, `waiting.prev` | open the next or previous session waiting on you |
| `session.next`, `session.prev` | open the session after or before the one shown, in the sidebar's order over every session, folded away or filtered out too, going round at the ends; the keys stay in the area they are in |
| `session.pick` | open the picker, to go to a session or choose where a new one works ([details](workspaces.md#going-to-a-session-or-starting-one)) |
| `session.new` | start a session, with the prompt ready to write its first message: in the selected row's worktree or directory from the sidebar, in the one the console shows from elsewhere |
| `session.new_reader` | start a session that only reads, as `session.new` does |
| `session.scratch` | start a scratch session, with the prompt ready: one that runs in a new directory of its own, for talking about something no project holds ([details](workspaces.md#scratch-sessions)) |
| `session.take_over` | take over an interactive session ([details](interactive-sessions.md)) |
| `session.stop` | end a session adroit runs after asking; Claude Code keeps its conversation, for `/resume` in a session started later |
| `session.rename` | ask for a new name for a session adroit runs, with its name to change; the sidebar shows it, and the session is sent Claude Code's `/rename` so its own interface says it too |
| `shell.open`, `shell.close` | open a shell in the worktree, close the current one: the one selected in the sidebar, or else the one shown |
| `neovim.open` | open Neovim, with your own configuration, in the worktree as `shell.open` opens a shell ([shells](shells.md)) |
| `scroll.down`, `scroll.up`, `scroll.half_down`, `scroll.half_up`, `scroll.top`, `scroll.bottom` | scroll the details panel; bound in the console, scroll it: Claude Code and other programs that read the mouse are sent the wheel, others are scrolled back through what scrolled off |
| `refresh` | read the sessions, and what git says of where they run, again now |
| `sidebar.toggle`, `details.toggle` | show or hide the sidebar, the details panel |
| `settings.open` | open the Settings window |
| `zoom.in`, `zoom.out`, `zoom.reset` | draw the window larger or smaller a step, or at the zoom the settings give |
| `filter` | narrow the sidebar to the sessions, shells, workspaces and worktrees whose names hold what is typed next; `Enter` keeps it, `Escape` clears it |
| `details.overview`, `details.changes`, `details.log` | show that sheet of the details panel ([details](details.md)), and the panel when it is hidden |
| `worktrees.toggle` | list every worktree git knows, those without sessions too, or only those with sessions again |
| `prompt.focus` | give the prompt the keys |
| `prompt.write` | give the prompt the keys, and the key pressed with them |
| `prompt.send` | send what the prompt holds |
| `prompt.history_prev`, `prompt.history_next` | put the prompt sent before or after the one shown in the prompt, to change and send again: those Claude Code recalls with its Up arrow where the shown tab works. In the prompt, while Neovim shows its completions or its command line, the key is Neovim's |
| `console.send` | send the key pressed to the session |
| `console.send <key>` | send the key named instead, as in `console.send Escape` |
| `console.send_next` | send the next key as it is, whatever it is bound to |
| `console.pass_every_key` | send every key as it is until `Escape`; `Ctrl+c` sends `Escape` meanwhile |
| `none` | nothing |

## adroit's own bindings

```toml
[global]
"Ctrl+w h" = "window.left"
"Ctrl+w ArrowLeft" = "window.left"
"Ctrl+w j" = "window.down"
"Ctrl+w ArrowDown" = "window.down"
"Ctrl+w k" = "window.up"
"Ctrl+w ArrowUp" = "window.up"
"Ctrl+w l" = "window.right"
"Ctrl+w ArrowRight" = "window.right"
"Ctrl+w n" = "tab.next"
"Ctrl+w p" = "tab.prev"
"Cmd+]" = "tab.next"
"Cmd+[" = "tab.prev"
"Cmd+w" = "tab.close"
"Cmd+W" = "window.close"
"Cmd+1" = "details.overview"
"Cmd+2" = "details.changes"
"Cmd+3" = "details.log"
"Ctrl+w ]" = "waiting.next"
"Ctrl+w [" = "waiting.prev"
"Ctrl+w N" = "session.next"
"Ctrl+w P" = "session.prev"
"Cmd+}" = "session.next"
"Cmd+{" = "session.prev"
"Cmd+p" = "session.pick"
"Ctrl+w ." = "console.send Ctrl+w"
"Cmd+n" = "session.new"
"Cmd+N" = "session.new_reader"
"Cmd+Alt+n" = "session.scratch"
"Cmd+s" = "shell.open"
"Cmd+Alt+e" = "neovim.open"
"Cmd+t" = "session.take_over"
"Cmd+Backspace" = "session.stop"
"Cmd+f" = "filter"
"Cmd+e" = "worktrees.toggle"
"Cmd+r" = "refresh"
"Cmd+," = "settings.open"
"Cmd+=" = "zoom.in"
"Cmd+Plus" = "zoom.in"
"Cmd+-" = "zoom.out"
"Cmd+0" = "zoom.reset"
"Cmd+b" = "sidebar.toggle"
"Cmd+Alt+b" = "details.toggle"

[sidebar]
"j" = "cursor.down"
"ArrowDown" = "cursor.down"
"k" = "cursor.up"
"ArrowUp" = "cursor.up"
"g g" = "cursor.first"
"G" = "cursor.last"
"Enter" = "cursor.open"
"l" = "cursor.open"
"o" = "cursor.open"
"L" = "cursor.show"
"Shift+Enter" = "cursor.show"
"Ctrl+Enter" = "cursor.show"
"h" = "cursor.fold"
"] w" = "waiting.next"
"[ w" = "waiting.prev"
"n" = "session.new"
"N" = "session.new_reader"
"s" = "shell.open"
"v" = "neovim.open"
"x" = "tab.close"
"X" = "session.stop"
"R" = "session.rename"
"t" = "session.take_over"
"i" = "prompt.write"
"a" = "prompt.write"
"r" = "refresh"
"/" = "filter"
"w" = "session.pick"
"!" = "worktrees.toggle"

[details]
"j" = "cursor.down"
"ArrowDown" = "cursor.down"
"k" = "cursor.up"
"ArrowUp" = "cursor.up"
"g g" = "cursor.first"
"G" = "cursor.last"
"Escape" = "cursor.fold"
"y" = "cursor.copy"
"Backspace" = "cursor.fold"
"Ctrl+d" = "scroll.half_down"
"Ctrl+u" = "scroll.half_up"

[console]
"Enter" = "console.send"
"ArrowUp" = "prompt.history_prev"
"ArrowDown" = "prompt.history_next"
"ArrowLeft" = "console.send"
"ArrowRight" = "console.send"
"Tab" = "console.send"
"Shift+Tab" = "console.send"
"F2" = "console.send"
"Ctrl+l" = "console.send"
"Ctrl+p" = "prompt.history_prev"
"Ctrl+n" = "prompt.history_next"
"Ctrl+o" = "console.send"
"Ctrl+t" = "console.send"
"Ctrl+_" = "console.send"
"Ctrl+-" = "console.send Ctrl+_"
"Ctrl+c" = "console.send Escape"
"g Ctrl+c" = "console.send Ctrl+c"
"Ctrl+g Ctrl+o" = "console.send_next"
"Ctrl+g i" = "console.pass_every_key"
"Ctrl+g Ctrl+i" = "console.pass_every_key"
"i" = "prompt.write"
"a" = "prompt.write"
"I" = "prompt.write"
"A" = "prompt.write"
"o" = "prompt.write"
"O" = "prompt.write"
"t" = "session.take_over"
"X" = "session.stop"

[prompt]
"Ctrl+Enter" = "prompt.send"
"Ctrl+p" = "prompt.history_prev"
"Ctrl+n" = "prompt.history_next"
```
