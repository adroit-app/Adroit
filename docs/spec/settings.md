# Settings

Adroit's settings are changed in its Settings window, opened with Command
comma or the gear in the window's header. Each change applies at once, to
every window, and is kept for the next run; there is nothing to save. The
file they are kept in can be edited by hand as well: a change saved there
applies within a second. The window also lists the key bindings in force, each
of which can be changed there
([keys](keys.md#changing-them-in-the-settings-window)).

## settings.toml

The settings are kept in `settings.toml`, in `$XDG_CONFIG_HOME/adroit/`, or in
`~/.config/adroit/` when `$XDG_CONFIG_HOME` is unset. Adroit
writes it whenever a setting changes, so comments in it are not kept. A
setting it does not name uses its default. Invalid or unreadable configuration
is reported in the main window and Settings, and Adroit refuses to overwrite
it until corrected. The window remains available, while sessions cannot start
sessions of their own. Sessions already running continue.
A value out of reason is brought into it.

The [settings reference](reference/settings.md) lists every setting, table by
table: its type, its default and what it does.

A program started through the shell is started by the shell `[shell]` names,
or your login shell, with the program and its arguments handed over as they
are; a POSIX shell or fish can do so. The Settings window changes the same
settings, a command written there as a shell would split it.

Choose an agent under Agents to change its launch options and the defaults
for its sessions. Only that agent's options are offered; changes
do not affect other agents or sessions already running. General sets the
agent new sessions start with. Programs holds the shell and Neovim options.

The Settings window offers the font among adroit's own, which is drawn when
none is named, and the families installed whose letters and digits are all
as wide, the console being a grid. A family the settings name that is not installed is offered as
such and kept, drawn in adroit's own meanwhile, until another is picked.

A theme is one of those adroit comes with, as many light as dark, each taken
from a theme for Vim or Neovim whose licence lets adroit ship it (Nightfox and
its sibling themes, TokyoNight, Catppuccin, Gruvbox, Kanagawa and Rose Pine
among them), or one of your own: a file in `themes/` beside `settings.toml`,
named as the theme, in Ghostty's theme format, so a theme file from Ghostty
works as it is. Of it, adroit reads the sixteen colours, the background, the
foreground and the cursor's colour. Your file of the same name as one adroit
comes with is used in its place. A theme whose background is darker than its
text is a dark one, any other a light one; one that cannot be read counts as
dark. Settings naming Adroit, the theme of adroit's own that earlier versions
offered, are drawn in Nightfox.

You choose a light theme and a dark theme, Dawnfox and Nightfox when you
choose none, and the appearance: System follows macOS, switching within a
second of macOS turning light or dark, as it does on its own when set to Auto;
Light and Dark keep to one theme. The Settings window offers for each only
the themes of its kind, and keeps a theme the settings name that is not among them in view until another is picked. The
theme follows macOS whether a window is open or not, so a session started with
none open takes the theme of the appearance in force. The whole window is
drawn in the theme's colours, and a new theme reaches the consoles already
open. A program started in a console is told the theme's colours from the
first time it asks, so one that picks its look by the background as it
starts, as Neovim does, picks the theme's.

An unknown setting makes the file unreadable, so a misspelt one is not
silently ignored. `naming_model` and `projects_dir`, which earlier versions
wrote, are read and dropped. `theme`, the one theme earlier versions named, is
read as the light or the dark theme by its kind, with the appearance kept to
that kind, so the look stays as it was; it is dropped the next time the file is
written.

## Neovim

The prompt and the console's reader each run in a Neovim of its own, apart
from the one you use every day and from each other: neither reads another's
configuration or plugins, and each keeps its history and swap files to
itself. The prompt's configuration is `prompt/nvim/init.lua` beside
`settings.toml`, and the reader's `console/nvim/init.lua`, each read when
it starts; their other files are kept in `adroit/prompt/nvim` and
`adroit/console/nvim` under Neovim's data and state folders. Each has its
`'background'` set to the theme's, light or dark, as it starts and whenever
the theme changes, so its colour scheme draws text that shows on the theme.

Either Neovim that quits is started again; the console read when its
reader quits is shown live. One that quits as it starts, as a
broken configuration makes it, or cannot be started is tried again after a
wait that grows while it keeps failing, the status line saying so; fixing
the configuration is enough to bring it back, without restarting adroit.
