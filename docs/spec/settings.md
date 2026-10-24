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

Agents holds the [profiles and connections](#profiles-and-connections),
which the window calls accounts. Programs holds the shell and Neovim options.

### Profiles in the Settings window

Under Agents the sidebar lists every profile not archived, marking the one
new sessions start from, then a new profile, then the accounts under the
agent they belong to: its account shared with the terminal, its own
environment, and every connection not archived. What is archived is
reached from an entry of its own. Each entry opens a page of its own, by
pointer or keyboard; when what a page shows is archived or gone, the page
of the default profile, or of the accounts, is shown instead.

A profile's page holds all of it: its name, its agent, its account with that
account's login, the options for its new sessions, only its agent's being
offered, and, folded away, its launch. There it is chosen for new sessions,
duplicated and archived. A new profile's id is made from its name and never
changes; a duplicate runs in the same account as the profile it copies.
Changing a profile's agent resets what it held for the agent before, its
options, its account and its command, and says so. The profile new sessions
start from cannot be archived, nor an archived one chosen for them. Changes
reach new sessions only.

A new account is made from a profile's page, where it is named and the
profile is moved to it at once, or from the accounts' page, where its agent
and folder can be chosen as well. It keeps its state in a folder of its own,
named by its id, under `adroit/connections` in the data folder
(`$XDG_DATA_HOME`, or `~/.local/share`), or in a folder chosen for it. Its
`home` is written then, once, as an absolute path through the links of the
folders that exist, and never rewritten; no two accounts share one. An
account's page shows its folder and the profiles that use it, and renames
and archives it. An account that profiles not archived use is not archived,
and the refusal names them. Archiving an account logs nothing out and leaves
its folder as it is.

An account is shown, on its page and on the page of each profile that uses
it, as logged in, not logged in, unknown, or with its agent's program not
found, as the program says when asked in that account, naming the profile
whose program was asked unless it is the page's own; adroit takes no login
for granted. It is asked when the account is first shown, when asked again,
and when its login terminal ends, never over and over. Open login terminal
starts a terminal, listed in the tree as any other, that runs the program's
own login in the account, the way a session there is started; the user logs
in in it. The account shared
with the terminal is the program as it runs outside adroit, so logging in or
out of it there changes it here too.

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

## Profiles and connections

A session starts from a profile, which names the agent it starts, the
connection the agent runs in, how the agent is launched and the agent's own
options for its sessions. Each `[[profiles]]` table is one profile, and
`profile` names the default profile, the one a new session starts from when
it names neither a profile nor an agent. A file that names no `profile`
takes its first profile not archived as the default.

```toml
profile = "claude-work"

[[profiles]]
id = "claude"
name = "Claude Personal"
agent = "claude"

[[profiles]]
id = "claude-work"
name = "Claude Work"
agent = "claude"
connection = "work"

[profiles.launch]
via_shell = true

[profiles.claude]
model = "opus"

[[profiles]]
id = "codex-review"
name = "Codex Review"
agent = "codex"

[profiles.codex]
sandbox_mode = "read-only"

[[connections]]
id = "work"
name = "Work"
agent = "claude"
home = "/Users/me/.adroit/claude-work"
```

A profile's `id` is how everything else refers to it, and it is kept when
the profile is renamed: renaming changes `name` alone. An id is lowercase
letters, digits and `-`, starts with a letter or a digit, and is at most 64
characters long; no two profiles share one. `agent` is `claude` or `codex`,
and the profile holds options for that agent alone: `[profiles.claude]` for
Claude Code, `[profiles.codex]` for Codex, either left out for the agent's
options with nothing chosen. `[profiles.launch]` says how the agent is
started, as `[neovim]` does for Neovim. An archived profile is kept, and is
offered for no new session; it cannot be the default.

Each agent has a default profile of its own: the default profile when it
starts that agent, else the agent's first profile not archived, in the order
the file lists them. A session that names an agent but no profile starts from
that agent's default profile; an agent with no profile starts as the profile
adroit comes with for it.

A connection is a state directory of one agent's program, which keeps its
login, history and settings there apart from every other. Each
`[[connections]]` table is one: its `id`, kept as a profile's is and never
one of the agents' names, its `name`, the `agent` whose program keeps its
state there, and its `home`, an absolute path fixed when the connection is
made. The program tells logins apart by the directory as it is written, so
`home` is written one way only: without `~`, `.` or `..`, and with no `/`
doubled or at its end. The agent's program logs in and keeps its
credentials in that directory itself; adroit keeps none. Every agent also
runs in its own environment, the program as it runs outside adroit, for
which adroit sets no state directory: a profile runs there when its
`connection` is left out or names the agent, as `claude`. A profile runs
only in a connection of its own agent.

A profile in a connection of its own starts its agent's program, and asks it
anything, with `home` as the program's state directory, which adroit makes
when it is missing and never puts anything in. None of the variables that
would carry another login, or move the program's state elsewhere, reaches
it, even when the shell it is started through sets them again. Its launch
cannot set them either, nor the state directory: a session of a profile
whose launch does is refused, naming the variable. In the agent's own
environment the launch sets what it likes.

A file that names no `profiles` holds the profiles adroit comes with, one
for each agent, named after the agent, which start it in its own environment
as its program's own settings say, with `claude` as the default profile
unless `profile` names another of them.

### Files written before profiles

Earlier versions kept one launch and one set of options for each agent, in
`claude`, `sessions`, `codex` and `codex_sessions`, and the agent new
sessions started with in `agent`. A file that names no `profiles` reads
them as the profiles adroit comes with, each in its agent's own environment,
`claude` from `claude` and `sessions` and `codex` from `codex` and
`codex_sessions`, with the profile of the agent `agent` names as the
default; sessions start as they did. The next time adroit writes the file,
it writes profiles in their place. A file that names `profiles`, `profile`
or `connections` beside any of them is refused, naming them.

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
