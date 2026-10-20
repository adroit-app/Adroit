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
it until corrected. The window remains available, while scheduling new Workers
and Leader restarts pauses. Sessions already running continue.
A value out of reason is brought into it.

```toml
# How large the window draws everything when it opens, from 0.67 to 1.75.
# Command 0 comes back to it.
zoom = 1.0

# The agent a new session starts with, claude or codex; a session not started
# yet can be switched to another for the current run of the window.
agent = "claude"

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

# Show a notification when a session starts waiting on you while the window
# is not in front.
notify_waiting = false

# Where scratch sessions run, `~` standing for your home folder; empty for
# adroit's own folder.
scratch_dir = ""

# How a shell tab's shell starts: the shell and its arguments, empty for your
# account's login shell started as a login shell, and variables set for it.
[shell]
command = []

[shell.env]

# How Claude Code starts for every session adroit runs: the program and the
# arguments it always gets ahead of adroit's own, empty for `claude`; whether
# your shell starts it, as an interactive login shell so that what its startup
# files set reaches it; and variables set for it.
[claude]
command = []
via_shell = false

[claude.env]

# How Codex starts, independently of Claude Code; empty for codex.
[codex]
command = []
via_shell = false

[codex.env]

# Codex's model and reasoning effort; empty for its own choice. Effort must
# be supported by the selected model and Codex version. Approval policy is
# empty, on-request or never. Sandbox is empty, read-only, workspace-write
# or danger-full-access. Empty values leave Codex's own settings in effect.
[codex_sessions]
model = ""
reasoning_effort = ""
approval_policy = ""
sandbox_mode = ""

# Active Worker attempts across runs, from 0 to 64. Zero pauses new starts
# without stopping Workers already running. Leaders and terminals do not count.
[orchestration]
max_workers = 4
# Active attempts for each independent request, also from 0 to 64.
# This limits Worker capacity, not access to a working directory.
max_workers_per_run = 2

# How a Neovim tab's Neovim starts, as Claude Code does: empty for `nvim`.
[neovim]
command = []
via_shell = false

[neovim.env]

# How Claude Code sessions run: the model it starts with, as
# Claude Code's `--model` takes it; its permission mode, one of `default`,
# `acceptEdits`, `plan` and `bypassPermissions`; and the tools denied it.
# What is left empty is Claude Code's own choice.
[sessions]
model = ""
permission_mode = ""
disallowed_tools = []

# Where the picker looks for directories to start a session in: the
# repositories ghq keeps, and the folders in each of `roots`, `~` standing for
# your home folder.
[picker]
ghq = true
roots = []

# How much the details panel reads and shows: the changes Log lists, the
# lines of a diff before it is cut, the links of each kind the overview keeps,
# the newest, up to 1000, and how many of them it shows before a row that
# shows them all.
[details]
log_commits = 50
diff_lines = 2000
links_kept = 100
links_shown = 5

# Machine resources shown in Usage across all tabs.
[usage]
cpu = true
gpu = true
memory = true
storage = true

# How the console takes keys and draws its cursor: which Option keys send Alt
# to what runs there, `both`, `left`, `right` or `none`, the others typing what
# macOS makes of the key; and whether the cursor blinks when the program lets
# it.
[console]
option_as_alt = "both"
cursor_blink = false
```

A program started through the shell is started by the shell `[shell]` names,
or your login shell, with the program and its arguments handed over as they
are; a POSIX shell or fish can do so. The Settings window changes the same
settings, a command written there as a shell would split it.

Choose an agent under Agents to change its launch options and the defaults
for its sessions. Only that agent's options are offered; changes
do not affect other agents or sessions already running. General sets the
agent new sessions start with. Programs holds the shell and Neovim options.

A theme is adroit's own, one of those it comes with (Nightfox, TokyoNight,
Catppuccin, Gruvbox, Dracula, Nord, Kanagawa, Rose Pine and GitHub Light among
them), or one of your own: a file in `themes/` beside `settings.toml`, named
as the theme, in Ghostty's theme format, so a theme file from Ghostty works as
it is. Of it, adroit reads the sixteen colours, the background, the
foreground and the cursor's colour. Your file of the same name as one adroit
comes with is used in its place. The whole window is drawn in the theme's
colours, and a new theme reaches the consoles already open. A program started
in a console is told the theme's colours from the first time it asks, so one
that picks its look by the background as it starts, as Neovim does, picks
the theme's.

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
