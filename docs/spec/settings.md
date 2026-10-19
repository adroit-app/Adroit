# Settings

Adroit's settings are changed in its Settings window, opened with Command
comma or the gear in the window's header. Each change applies at once, to
every window, and is kept for the next run; there is nothing to save. The
window also lists the key bindings in force, each of which can be changed there
([keys](keys.md#changing-them-in-the-settings-window)).

## settings.toml

The settings are kept in `settings.toml`, in `$XDG_CONFIG_HOME/adroit/`, or in
`~/.config/adroit/` when `$XDG_CONFIG_HOME` is unset. Adroit
writes it whenever a setting changes, so comments in it are not kept. A
setting it does not name is as designed, and a file that cannot be read
leaves every setting as designed. A value out of reason is brought into it.

```toml
# How large the window draws everything when it opens, from 0.67 to 1.75.
# Command 0 comes back to it.
zoom = 1.0

# The fewest lines the prompt shows, and the most it grows to before it
# scrolls inside.
prompt_min_rows = 3
prompt_max_rows = 12

# The font the console and the prompt are drawn in, as the system names its
# family, ahead of the built-in ones; empty for those alone. Its size runs
# from 8 to 32 points and its weight from 100 to 900. A line is as tall as the
# font asks, as in a terminal.
font_family = ""
font_size = 13.0
font_weight = 400

# The console's colours, by the name of a theme.
theme = "Adroit"

# Keep the Mac from idle sleep while a session or a shell runs, the window
# open or not; the display still sleeps.
keep_awake = true
```

A theme is adroit's own, one of those it comes with (Nightfox, TokyoNight,
Catppuccin, Gruvbox, Dracula, Nord, Kanagawa, Rose Pine and GitHub Light among
them), or one of your own: a file in `themes/` beside `settings.toml`, named
as the theme, in Ghostty's theme format, so a theme file from Ghostty works as
it is. Of it, adroit reads the sixteen colours, the background, the
foreground and the cursor's colour. Your file of the same name as one adroit
comes with is used in its place. The whole window is drawn in the theme's
colours, and a new theme reaches the consoles already open.

An unknown setting makes the file unreadable, so a misspelt one is not
silently ignored. `naming_model` and `projects_dir`, which earlier versions
wrote, are read and dropped.

## Neovim

The prompt and the console's reader each run in a Neovim of its own, apart
from the one you use every day and from each other: neither reads another's
configuration or plugins, and each keeps its history and swap files to
itself. The prompt's configuration is `prompt/nvim/init.lua` beside
`settings.toml`, and the reader's `console/nvim/init.lua`, each read when
it starts; their other files are kept in `adroit/prompt/nvim` and
`adroit/console/nvim` under Neovim's data and state folders.
