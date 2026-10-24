# Keys

Every key adroit handles itself runs a named action. adroit ships bindings of
its own, and a file you write changes, adds to or takes away from them. The
prompt's keys are Neovim's and are set up in Neovim; a dialog that asks
before ending something always answers to `y`, `Enter`, `n` and `Escape`, and
attachment selection, the picker and the list of past conversations have their
own keys ([the tree](tree.md#going-to-a-session-or-starting-one),
[sessions](sessions.md#resuming-a-past-conversation)).
`F1` shows every binding in force wherever the keys are.

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
`project.new`, `project.edit`, `workspace.pick`, `details.files`,
`details.diff` or `session.take_over`, actions earlier versions had, is read and dropped. The area
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
"X" = "none"
"Ctrl+x" = "session.stop"

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
| `filer` | a file tree: the tree's, before its own while a file or a directory is selected, and the picker's |
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

The [key reference](reference/keys.md#actions) lists every action a binding
can name: what it does, what the menu bar calls it, and the keys adroit binds
it to.

## adroit's own bindings

The [key reference](reference/keys.md#default-bindings) holds them, written
as a file of bindings would write them.
