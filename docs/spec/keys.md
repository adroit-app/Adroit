# Keys

Every key adroit handles itself runs a named action. adroit ships bindings of
its own, and a file you write changes, adds to or takes away from them. The
prompt's keys are Neovim's and are set up in Neovim; the dialog that asks
before taking over a session always answers to `y`, `Enter`, `n` and `Escape`,
and attachment selection and the picker have their own keys ([workspaces](workspaces.md#going-to-a-session-or-starting-one)).

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

What the keys do to a row of the tree or a tab of the content is also offered by
right-clicking it, each entry with the keys bound to it. A tab closes with its
`×`, a window takes the keys when clicked, a border between windows is dragged
to resize them, a workspace's or worktree's row starts a session there with its `+`, and
the buttons beside the tree's filter list every worktree and start a session.

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
`details.diff`, actions earlier versions had, is read and dropped. The area
earlier versions called `sidebar` is read as `tree`, and `sidebar.toggle` as
`tree.toggle`. The window
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
[tree]
"Ctrl+n" = "cursor.down"
"Ctrl+p" = "cursor.up"
"T" = "none"
"t" = "session.take_over"

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
| `tree` | the tree, and the details panel after its own |
| `details` | the details panel |
| `console` | the console while it shows a session |
| `prompt` | the prompt, before Neovim |
| `draft` | before the area's bindings while a new session is shown, including the prompt in insert mode |
| `viewer` | a viewer in the focused window |

A key that reaches a program in the console is sent as the program asked,
as felis sends it: with the Kitty keyboard protocol once it asks for it, as
xterm sends it otherwise.
A shell's or Neovim's window takes every key that is not a `global` binding,
and shows no prompt below it: a window move does not stop there, and the
`prompt` actions do nothing. A
session waiting on a permission prompt or a question does too, as its answer,
from the prompt as well as the console; the prompt is Neovim's again once it
is answered. So does a session showing a dialog of its agent's own in place
of its input box, or above it while the agent works, such as the picker `/model`
opens, until the dialog closes.
The dialog is told from what the console shows; one adroit does not
recognise still takes `console.pass_every_key`.
A session still starting, whose program has reported nothing and not yet
drawn its input box, also takes every key, such as Claude Code asking whether
to trust the folder. One that has drawn its input box is no longer starting,
even when what its program reported was lost.
The mouse wheel scrolls the console back in either case, and what is sent to
it shows the bottom again.
A key the `console` area does not bind, in a session's console, starts
reading it: what the console holds, scrolled off and on screen, colours kept,
is read in Neovim, with your own Vim keys for moving, scrolling, searching and
selecting, and a yank copies to the clipboard. It follows what the session
writes meanwhile. `q`, `Esc` or `G` in normal mode goes back to the console live,
and `i`, `a` and `o` to the prompt; `Ctrl-q` sequences stay `global`, and
one that ends no binding does nothing. The reader is for reading only: `:`,
Ex mode, quitting and Vim's window commands do nothing in it, and a window or
a tab a mapping or a plugin opens in it closes at once.
A program that scrolls what it shows itself, as Claude Code does on the
alternate screen, leaves nothing scrolled off to read: moving or scrolling past the reader's
first or last line scrolls the program instead, and the reader follows; `gg` scrolls it to its start. Leaving the reader
for the live console, or moving to the prompt, returns the session to its
latest output.
In the prompt, `Ctrl+c` sends `Escape` to the agent, `Shift+Tab` is sent to it
as it is, and `Tab` on an empty prompt takes the prompt the agent suggests,
in insert mode too; both are Neovim's while it shows its completions or its
command line, or the prompt has no session to send them to. `global` bindings
apply only outside Neovim's insert mode, but for `Mod` and Command keys and
the prefix.

### Keys

A key is written as its modifiers, then its name, joined by `+`: `Ctrl+q`,
`Shift+Tab`, `Ctrl+Alt+x`, `Mod+p`. The modifiers are `Mod`, `Cmd`, `Ctrl`,
`Alt` and `Shift`. `Mod` is Command on macOS and Ctrl with Shift elsewhere,
so one binding serves every platform; the window writes it as the keys it is
there. Off macOS, a character other than a letter keeps the Shift `Mod`
holds: `Mod+]` is `Ctrl+Shift+]`. Command or `Mod` with `c`, `v`, `x`, `a`,
`z`, `q`, `h` and `m` stays with the menu: editing, quitting, hiding and
minimizing.
A key held with `Mod` or Command reaches the `global` bindings wherever the
keys are, the prompt's insert mode and a console passing its keys on
included.
A key is a single character, `Space`, `Plus`, `F1` to `F12`, or one of
`Enter`, `Escape`, `Tab`, `Backspace`, `Delete`, `Insert`, `Home`, `End`,
`PageUp`, `PageDown`, `ArrowUp`, `ArrowDown`, `ArrowLeft` and `ArrowRight`. A
character carries its own case, so `G` is Shift and g, and `Shift+g` means the
same. Any other character is written as the one Shift types, `!` rather than
`Shift+1`.

A sequence is keys separated by spaces, pressed one after another: `g g`,
`Ctrl+q h`. When the keys pressed so far complete one binding and start
another, the complete one runs.

A key held with Ctrl that begins a `global` sequence, as `Ctrl+q` does, is a
prefix: it reaches the `global` bindings wherever the keys are, as a Command
key does. In a sequence begun with Ctrl held, a later key held with Ctrl too
means what it means without, as after Vim's `Ctrl-w`, unless the sequence is
bound with it so: `Ctrl+q Ctrl+h` is `Ctrl+q h`. Tab is the exception;
`Ctrl+q Ctrl+Tab` is nothing.

### Actions

| Action | Does |
| --- | --- |
| `window.left`, `window.down`, `window.up`, `window.right` | move to the window in that direction: the tree lies left of the content's windows and the details panel right of them, and a session's prompt below its console |
| `window.next`, `window.prev` | move to the window after or before the one the keys are in, from the tree through the content's windows, each console before its prompt, to the details panel, going round at the ends |
| `window.top`, `window.bottom` | move to the first or the last of these, the tree or the details panel when shown |
| `window.back` | move back to the window the keys were in before |
| `window.split`, `window.vsplit` | split the focused window one above the other or side by side, the new window, below or right of it, showing the same ([content](content.md)) |
| `window.new` | split the focused window one above the other and start writing a new session in the new one, in the worktree the focused window shows |
| `window.quit` | close the focused window, and its tab when it was the last; what it showed runs on |
| `window.only` | close every other window of the tab |
| `window.move_left`, `window.move_down`, `window.move_up`, `window.move_right` | move the focused window to that side of its tab, as tall or as wide as the tab |
| `window.exchange` | exchange the focused window with the next one in its split, or the one before the last |
| `window.rotate` | rotate the windows of the focused window's split forward, the last becoming the first |
| `window.equalize` | give the windows of every split the same size |
| `window.wider`, `window.narrower`, `window.taller`, `window.shorter` | make the focused window a step larger or smaller |
| `window.maximize_width`, `window.maximize_height` | make the focused window as wide or as tall as the others allow |
| `window.to_tab` | move the focused window to a new tab, unless it is the only one of its tab |
| `tab.next`, `tab.prev` | show the next or previous tab of the area the keys are in: the details panel's sheets there, elsewhere the content's tabs |
| `tab.go <n>` | show the content's tab numbered `n`, from 1 to 9, or the last when there are fewer |
| `tab.last` | show the content's tab shown before the one shown now |
| `tab.close` | close the content's tab shown and every window in it; agents and terminals keep running and remain reachable from the tree |
| `window.close` | close the window, which ends adroit; its sessions run on |
| `cursor.down`, `cursor.up`, `cursor.first`, `cursor.last` | move the selection in the tree; in the details panel, in the list it shows, or scroll what else it shows |
| `cursor.go` | go to a window showing the selected session, shell, worktree or file, in any tab, or else open it as `cursor.open` does; in the details panel, open the selected row |
| `cursor.open` | open the selected session, shell, worktree or file in the focused window, in place of what it shows, and move to it, or to the prompt of a session not started yet; on a folded row, a worktree or a session holding others too, unfold it instead; fold or unfold a workspace, a heading or a directory; in the details panel, open the selected row |
| `cursor.split`, `cursor.vsplit` | open the selected session, shell, worktree or file as `cursor.open` does, in a new window split off the focused one, below it or beside it |
| `cursor.tab` | open the selected session, shell, worktree or file as `cursor.open` does, in a new tab after the one shown |
| `cursor.show` | open the selected session, shell, worktree or file in the focused window as `cursor.open` does, with the keys staying in the tree; fold or unfold a workspace, a heading or a directory; in the details panel, open the selected row |
| `cursor.close` | close every window showing the selected session, shell or Neovim, or the selected file in its viewer or its Neovim; what they showed runs on |
| `cursor.fold` | fold the selected node, or on a row that is not unfolded go up to the row it sits under; in the details panel, go back out to the list what it shows was opened from |
| `cursor.copy` | copy the path of the selected file, worktree or workspace, or the name of the selected session, shell or Neovim; in the details panel, the selected link of the overview: its address, or its file's path |
| `cursor.reveal` | in the details panel, show the selected file of the overview in the Finder |
| `waiting.next`, `waiting.prev` | open the next or previous session waiting on you |
| `session.next`, `session.prev` | open the session after or before the one shown, in the tree's order over every session, folded away or filtered out too, going round at the ends; the keys stay in the area they are in |
| `workspace.next`, `workspace.prev` | open a session of the workspace after or before the one shown, in the tree's order over every workspace holding a session, folded away or filtered out too, going round at the ends: the session last shown there since the window opened, or else its first; the keys stay in the area they are in |
| `session.pick` | open the picker, to go to a session or choose where a new one works ([details](workspaces.md#going-to-a-session-or-starting-one)) |
| `session.new` | start a session, with the prompt ready to write its first message: in the selected row's worktree or directory from the tree, in the one the focused window shows from elsewhere |
| `session.scratch` | start a scratch session, with the prompt ready: one that runs in a new directory of its own, for talking about something no project holds ([details](workspaces.md#scratch-sessions)) |
| `session.agent_next`, `session.agent_prev` | while a session not started yet is shown, switch the agent it is to run to the next or the previous one ([details](sessions.md#agents)) |
| `session.take_over` | take over an interactive session ([details](interactive-sessions.md)) |
| `session.stop` | end a session adroit runs after asking; Claude Code keeps its conversation, for `/resume` in a session started later |
| `session.rename` | edit the name of a session adroit runs in place; the tree shows it, and the session is sent Claude Code's `/rename` so its own interface says it too |
| `shell.open` | open a shell in the worktree |
| `terminal.stop` | end the terminal selected in the tree, or else the one shown |
| `neovim.open` | open Neovim, with your own configuration, in the worktree as `shell.open` opens a shell ([shells](terminals.md)) |
| `scroll.down`, `scroll.up`, `scroll.half_down`, `scroll.half_up`, `scroll.top`, `scroll.bottom` | scroll the details panel; bound in the console, scroll it: Claude Code and other programs that read the mouse are sent the wheel, others are scrolled back through what scrolled off; bound in a viewer, scroll what it shows |
| `viewer.zoom_in`, `viewer.zoom_out`, `viewer.zoom_fit` | in a viewer, draw an image, a PDF or an HTML page larger, smaller, or fitting the window ([viewers](viewers.md)) |
| `viewer.page_next`, `viewer.page_prev` | in a viewer, show a PDF's next or previous page |
| `viewer.play`, `viewer.back`, `viewer.forward` | in a viewer, play or pause a movie, or seek it back or forward |
| `viewer.copy_path` | in a viewer, copy the path of the file it shows |
| `refresh` | read the sessions, and what git says of where they run, again now |
| `tree.toggle`, `details.toggle` | show or hide the tree, the details panel |
| `settings.open` | open the Settings window |
| `daemon.restart` | restart the sessions' daemon after asking, ending every session it holds ([details](sessions.md#the-daemon)) |
| `zoom.in`, `zoom.out`, `zoom.reset` | draw the window larger or smaller a step, or at the zoom the settings give |
| `filter` | narrow the tree to the sessions, shells, workspaces and worktrees whose names hold what is typed next; `Enter` keeps it, `Escape` clears it |
| `details.overview`, `details.changes`, `details.log` | show that sheet of the details panel ([details](details.md)), and the panel when it is hidden |
| `worktrees.toggle` | list every worktree git knows, those without sessions too, or only those with sessions again |
| `prompt.focus` | give the prompt the keys |
| `prompt.write` | give the prompt the keys, and the key pressed with them |
| `prompt.send` | send what the prompt holds |
| `prompt.images` | select an attached image |
| `prompt.take_suggestion` | on an empty prompt, send the key pressed to the session and then bring the session's input into the prompt, so a prompt Claude Code suggests is taken to edit; an agent whose input cannot be taken back gets the key alone. On a prompt holding text or images, or while Neovim shows its completions or its command line, the key is Neovim's |
| `prompt.history_prev`, `prompt.history_next` | put the prompt sent before or after the one shown in the prompt, to change and send again: those Claude Code recalls with its Up arrow where the focused window's session works. In the prompt, while Neovim shows its completions or its command line, the key is Neovim's |
| `console.send` | send the key pressed to the session |
| `console.send <key>` | send the key named instead, as in `console.send Escape` |
| `console.send_next` | send the next key as it is, whatever it is bound to |
| `console.pass_every_key` | send every key as it is until `Escape`; `Ctrl+c` sends `Escape` meanwhile |
| `none` | nothing |

## adroit's own bindings

```toml
[global]
"Ctrl+q h" = "window.left"
"Ctrl+q ArrowLeft" = "window.left"
"Ctrl+q j" = "window.down"
"Ctrl+q ArrowDown" = "window.down"
"Ctrl+q k" = "window.up"
"Ctrl+q ArrowUp" = "window.up"
"Ctrl+q l" = "window.right"
"Ctrl+q ArrowRight" = "window.right"
"Ctrl+q w" = "window.next"
"Ctrl+q Ctrl+q" = "window.next"
"Ctrl+q W" = "window.prev"
"Ctrl+q t" = "window.top"
"Ctrl+q b" = "window.bottom"
"Ctrl+q p" = "window.back"
"Ctrl+q v" = "window.vsplit"
"Ctrl+q s" = "window.split"
"Ctrl+q n" = "window.new"
"Ctrl+q c" = "window.quit"
"Ctrl+q q" = "window.quit"
"Ctrl+q o" = "window.only"
"Ctrl+q H" = "window.move_left"
"Ctrl+q J" = "window.move_down"
"Ctrl+q K" = "window.move_up"
"Ctrl+q L" = "window.move_right"
"Ctrl+q x" = "window.exchange"
"Ctrl+q r" = "window.rotate"
"Ctrl+q =" = "window.equalize"
"Ctrl+q <" = "window.narrower"
"Ctrl+q >" = "window.wider"
"Ctrl+q Plus" = "window.taller"
"Ctrl+q -" = "window.shorter"
"Ctrl+q _" = "window.maximize_height"
"Ctrl+q |" = "window.maximize_width"
"Ctrl+q T" = "window.to_tab"
"Ctrl+q Tab" = "tab.next"
"Ctrl+q Shift+Tab" = "tab.prev"
"Ctrl+q 1" = "tab.go 1"
"Ctrl+q 2" = "tab.go 2"
"Ctrl+q 3" = "tab.go 3"
"Ctrl+q 4" = "tab.go 4"
"Ctrl+q 5" = "tab.go 5"
"Ctrl+q 6" = "tab.go 6"
"Ctrl+q 7" = "tab.go 7"
"Ctrl+q 8" = "tab.go 8"
"Ctrl+q 9" = "tab.go 9"
"Ctrl+q g Tab" = "tab.last"
"Mod+]" = "tab.next"
"Mod+[" = "tab.prev"
"Mod+w" = "tab.close"
"Cmd+W" = "window.close"
"Mod+1" = "details.overview"
"Mod+2" = "details.changes"
"Mod+3" = "details.log"
"Ctrl+q ]" = "waiting.next"
"Ctrl+q [" = "waiting.prev"
"Ctrl+q N" = "session.next"
"Ctrl+q P" = "session.prev"
"Ctrl+q }" = "workspace.next"
"Ctrl+q {" = "workspace.prev"
"Mod+p" = "session.pick"
"Ctrl+q ." = "console.send Ctrl+q"
"Mod+n" = "session.new"
"Mod+s" = "shell.open"
"Mod+e" = "neovim.open"
"Mod+t" = "session.take_over"
"Mod+Backspace" = "session.stop"
"Mod+f" = "filter"
"Mod+r" = "refresh"
"Mod+," = "settings.open"
"Mod+=" = "zoom.in"
"Mod+Plus" = "zoom.in"
"Mod+-" = "zoom.out"
"Mod+0" = "zoom.reset"
"Mod+b" = "tree.toggle"
"Mod+d" = "details.toggle"

[draft]
"Ctrl+g a" = "session.agent_next"
"Ctrl+g A" = "session.agent_prev"

[tree]
"j" = "cursor.down"
"ArrowDown" = "cursor.down"
"k" = "cursor.up"
"ArrowUp" = "cursor.up"
"g g" = "cursor.first"
"G" = "cursor.last"
"Enter" = "cursor.go"
"o" = "cursor.open"
"l" = "cursor.open"
"s" = "cursor.split"
"v" = "cursor.vsplit"
"t" = "cursor.tab"
"L" = "cursor.show"
"Shift+Enter" = "cursor.show"
"Ctrl+Enter" = "cursor.show"
"h" = "cursor.fold"
"n" = "session.new"
"$" = "shell.open"
"e" = "neovim.open"
"T" = "session.take_over"
"x" = "cursor.close"
"X" = "session.stop"
"R" = "session.rename"
"y" = "cursor.copy"
"i" = "prompt.write"
"a" = "prompt.write"
"r" = "refresh"
"/" = "filter"
"w" = "session.pick"
"!" = "worktrees.toggle"
"] w" = "waiting.next"
"[ w" = "waiting.prev"
"}" = "workspace.next"
"{" = "workspace.prev"

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
"T" = "session.take_over"
"X" = "session.stop"

[prompt]
"Mod+i" = "prompt.images"
"Tab" = "prompt.take_suggestion"
"Shift+Tab" = "console.send"
"Ctrl+c" = "console.send Escape"
"Ctrl+Enter" = "prompt.send"
"Ctrl+p" = "prompt.history_prev"
"Ctrl+n" = "prompt.history_next"

[viewer]
"j" = "scroll.down"
"ArrowDown" = "scroll.down"
"k" = "scroll.up"
"ArrowUp" = "scroll.up"
"Ctrl+d" = "scroll.half_down"
"Ctrl+u" = "scroll.half_up"
"g g" = "scroll.top"
"G" = "scroll.bottom"
"Plus" = "viewer.zoom_in"
"=" = "viewer.zoom_in"
"-" = "viewer.zoom_out"
"0" = "viewer.zoom_fit"
"J" = "viewer.page_next"
"K" = "viewer.page_prev"
"Space" = "viewer.play"
"h" = "viewer.back"
"l" = "viewer.forward"
"y" = "viewer.copy_path"
"q" = "window.quit"

```
