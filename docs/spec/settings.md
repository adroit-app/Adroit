# Settings

Adroit's settings are changed in its Settings window, opened with Command
comma or the gear in the window's header. Each change applies at once, to
every window, and is kept for the next run; there is nothing to save. The
file they are kept in can be edited by hand as well: a change saved there
applies within a second. The window also lists the key bindings in force, each
of which can be changed there
([keys](keys.md#changing-them-in-the-settings-window)). Its About page, set
apart from the settings, tells which build of Adroit is running, opens the
files the settings and key bindings are kept in, and leads to the source and
to where problems are reported.

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

Agents holds the [profiles](#profiles). Programs holds the shell and Neovim
options.

### Profiles in the Settings window

Under Agents the sidebar lists every profile, marking the one new sessions
start from, then a new profile. Each entry opens a page of its own, by
pointer or keyboard; when the profile a page shows is gone, the page of the
default profile is shown instead.

A profile's page holds all of it: its name, its agent, its login, the
options for its new sessions, only its agent's being offered, and, folded
away, its launch. There it is chosen for new sessions, duplicated and
deleted. A new profile's id is made from its name and never changes; the id
of a profile deleted can be taken again.
Changing a profile's agent resets what it held for the agent before, its
options, its login and its command, and says so. The profile new sessions
start from cannot be deleted. Changes reach new sessions only.

A profile's login is your terminal's or the profile's own. Your terminal's
is its agent's program as it runs outside adroit, so logging in or out of it
there changes it here too, and every profile that uses your terminal's
shares it. The profile's own is a login, history and settings of its own,
kept in a folder of its own, named by its id, under `adroit/profiles` in the
data folder (`$XDG_DATA_HOME`, or `~/.local/share`). Its `home` is written
when it is chosen, once, as an absolute path through the links of the
folders that exist, and never rewritten; the page shows it. A folder that
is there already, as one a deleted profile left, is never taken for a new
login: the new one is numbered past it. A new profile
uses your terminal's login; a duplicate uses your terminal's when the
profile it copies does, and else a login of its own, not logged in yet.
Choosing your terminal's again logs nothing out and leaves the folder as it
is; choosing the profile's own again before Settings closes uses it again,
and after, a new one.

Deleting a profile asks first. A profile on your terminal's login is just
deleted; adroit touches nothing of its agent's own environment. One with a
login of its own asks each time whether to delete its folder too, which is
left unchosen and warns that the login, history and settings in it are lost
and that the sessions that ran in it can no longer be resumed. Left
unchosen, the folder stays as it is and its path is shown once the profile
is deleted. A folder adroit did not make, outside `adroit` in the data
folder, is never deleted; the path is shown to delete by hand. Deleting the
folder is refused, and the profile kept, while sessions run in it, naming
them. Sessions that started from a profile keep running after it is
deleted, and are resumed, as the settings they started with say.

The login is shown as logged in, not logged in, unknown, or with its agent's
program not found, as the profile's program says when asked there; adroit
takes no login for granted. It is asked when the page is first shown, when
asked again, and when its login terminal ends, never over and over. Open
login terminal starts a terminal, listed in the tree as any other, that runs
the program's own login where the profile's sessions run, the way a session
of it is started; the user logs in in it. Pi logs in only from its own
interface, so its login terminal opens Pi with `/login` typed in. OpenCode's
login terminal runs its own `auth login`, which asks which provider to
connect. While that terminal is open the login is waited for: asking again
ends the wait only when the program says it is logged in, and cancelling
ends the terminal, as closing it does, and asks again.

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

## Profiles

A session starts from a profile, which names the agent it starts, where the
agent keeps its login, how the agent is launched and the agent's own options
for its sessions. Each `[[profiles]]` table is one profile, and `profile`
names the default profile, the one a new session starts from when it names
neither a profile nor an agent. A file that names no `profile` takes its
first profile as the default.

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
home = "/Users/me/.local/share/adroit/profiles/claude-work"

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

[[profiles]]
id = "pi-local"
name = "Pi Local"
agent = "pi"
home = "/Users/me/.local/share/adroit/profiles/pi-local"

[profiles.pi]
model = "ollama/qwen3:8b"
thinking = "low"
tools = ["read", "grep", "find", "ls", "bash"]
```

A profile's `id` is how everything else refers to it, and it is kept when
the profile is renamed: renaming changes `name` alone. An id is lowercase
letters, digits and `-`, starts with a letter or a digit, and is at most 64
characters long; no two profiles share one. `agent` is `claude`, `codex`,
`pi` or `opencode`, and the profile holds options for that agent alone:
`[profiles.claude]` for Claude Code, `[profiles.codex]` for Codex,
`[profiles.pi]` for Pi, `[profiles.opencode]` for OpenCode, each left out
for the agent's options with nothing chosen. `[profiles.launch]` says how
the agent is started, as `[neovim]` does for Neovim.

Each agent has a default profile of its own: the default profile when it
starts that agent, else the agent's first profile, in the order
the file lists them. A session that names an agent but no profile starts from
that agent's default profile; an agent with no profile starts as the profile
adroit comes with for it.

A profile without `home` runs in its agent's own environment, the program as
it runs outside adroit, shared with your terminal, for which adroit sets no
state directory. `home` gives the profile a login of its own: the directory
its agent's program keeps its login, history and settings in, apart from
every other. It is an absolute path, fixed when the login is made. The
program tells logins apart by the directory as it is written, so `home` is
written one way only: without `~`, `.` or `..`, and with no `/` doubled or
at its end; no two profiles name the same one. The agent's
program logs in and keeps its credentials in that directory itself; adroit
keeps none.

A profile with a login of its own starts its agent's program, and asks it
anything, with `home` as the program's state directory, which adroit makes
when it is missing and never puts anything in. None of the variables that
would carry another login, or move the program's state elsewhere, reaches
it, even when the shell it is started through sets them again. Its launch
cannot set them either, nor the state directory: a session of a profile
whose launch does is refused, naming the variable. In the agent's own
environment the launch sets what it likes.

Pi keeps more than its logins in its state directory, so a Pi profile with a
login of its own also keeps its sessions there, whatever a project's own Pi
settings say, and loads only the skills in the `skills` folder of its
`home`, beside the one Adroit gives every session: none of those every Pi of
the user shares. The context files of the directory it works in, such as
`AGENTS.md`, are read all the same. Pi tells whether it can reach one
provider, never whether it is logged in at all, so a Pi profile's login is
shown as logged in when the provider of its model is ready, and as not
logged in when it is not.

OpenCode keeps its logins, conversations, configuration and caches in folders
of its own, so an OpenCode profile with a login of its own keeps all of them,
and the output of the commands it runs, in its `home`; the commands a session
runs are given back the folders of the environment Adroit runs in. OpenCode
connects every provider whose key it finds in its environment, so in a login
of its own it gets only the variables that locate the user, the terminal and
the network, Adroit's own, and those the profile's launch sets: a key or a
toolchain's folder its sessions need is set there. The user's home stays the
user's, so the skills OpenCode finds there are every profile's. An OpenCode
login is shown as logged in once OpenCode lists any provider connected in
it, a key it found in the environment among them; one shown as not logged
in says that free and local models need no login.

A file that names no `profiles` holds the profiles adroit comes with, one
for each agent, named after the agent, which start it in its own environment
as its program's own settings say, with `claude` as the default profile
unless `profile` names another of them.

### Local models for Pi

Pi reaches a model server of your own, such as Ollama, as a provider named
in `models.json` in its state directory: the profile's `home`, or
`~/.pi/agent` in Pi's own environment. Ollama serves the OpenAI chat API, so
it is named with `openai-completions` as its API, its address with `/v1`,
any key, which Ollama does not check, and each model by the name Ollama
gives it:

```json
{
  "providers": {
    "ollama": {
      "baseUrl": "http://127.0.0.1:11434/v1",
      "api": "openai-completions",
      "apiKey": "ollama",
      "models": [{ "id": "qwen3:8b", "contextWindow": 32768 }]
    }
  }
}
```

A profile then names the model as `ollama/qwen3:8b`, and its login is
shown as logged in. Pi lists only the models named there, and Adroit

### Local models for OpenCode

OpenCode finds an Ollama running on this machine by itself. One elsewhere, or
any other OpenAI-compatible server, is named as a provider in OpenCode's
configuration, `opencode.json` in the `opencode` folder of the `config` folder
in the profile's `home`, or of `~/.config` in OpenCode's own environment:

```json
{
  "providers": {
    "ollama": {
      "settings": { "baseURL": "http://gpu-host:11434/v1" }
    },
    "lab": {
      "name": "Lab",
      "package": "@opencode/ai/providers/openai-compatible",
      "settings": { "baseURL": "http://127.0.0.1:8000/v1", "apiKey": "none" },
      "models": { "qwen3-coder": { "name": "Qwen 3 Coder" } }
    }
  }
}
```

A profile then names the model as `ollama/qwen3:8b` or `lab/qwen3-coder`.
Adroit neither starts the server nor fetches a model for it.

### Archived profiles

Earlier versions kept profiles no longer used with `archived = true`. Such a
profile is read as if it were not in the file, and is dropped the next time
adroit writes it; its folder is left as it is.

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

### Files written with connections

Earlier versions kept logins apart from profiles, each `[[connections]]`
table one with an `id`, a `name`, an `agent` and a `home`, and a profile
named the one it ran in by `connection`. Such a
file is read with each profile given the `home` of the connection it names,
so its login, history and settings stay where they were; a profile that
names none, or its agent's key, runs in its agent's own environment as
before. When several profiles name one connection, the first of them in the
file keeps its folder, and the others run in their agent's own environment:
the main window and Settings name them until the file is written again, and each can be given a login of its own, logged in anew.
No folder is moved or removed. A connection the file names that it does not
define, one of another agent, one whose `home` is not written the one way,
and a profile that names both `connection` and `home` make the file
unreadable, as before. The next time adroit writes the file, it writes
`home` in the profiles and no `connections`.

Sessions recorded before keep running, and are resumed, in the folder they
started in, whichever profile has it since.

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
The Settings window's Prompt and Console pages name each configuration's
folder, copy its path, show it in the Finder, and edit its `init.lua` in a
Neovim of the main window, as a file of the tree is edited; the folder is made
when it is not there yet.

A configuration can change everything adroit sets in these Neovims except
what the window relies on. The prompt and the reader have no status line, tab
line, window bar or command line, ignore no events, keep the theme's
`'background'`, and hold their text in a scratch buffer of the filetype
`adroit-prompt` or `adroit-console`. The prompt's text can always be changed,
and completion offers what matches without putting it in the text. The
reader's text stays as the console draws it: unwrapped, unfolded, with nothing
concealed or beside it, and unable to be changed. Its keys that would leave
the reader, `:` and Ex mode, quitting, and `<C-w>`, stay without effect. These
are set again whenever a configuration, a filetype plugin or a plugin changes
them. The keys the window itself gives these Neovims, to start writing or to
scroll the reader, work whatever the configuration maps them to. The rest, such as line numbers,
completion options or messages, is set before the configuration is read, so
setting it in `init.lua` or a filetype plugin is enough. `g:adroit` is set
before the configuration too, for a configuration shared with the everyday
Neovim to tell where it runs.

Either Neovim that quits is started again; the console read when its
reader quits is shown live. One that quits as it starts, as a
broken configuration makes it, or cannot be started is tried again after a
wait that grows while it keeps failing, the status line saying so; fixing
the configuration is enough to bring it back, without restarting adroit.
