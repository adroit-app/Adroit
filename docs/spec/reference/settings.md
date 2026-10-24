# Settings reference

<!-- Written by `just references` from the code; edit the code, not this file. -->

Every setting `settings.toml` holds, table by table: its type, what it is
when the file does not name it, and what it does
([settings](../settings.md)).

## The top level

| Setting | Type | Default | Description |
| --- | --- | --- | --- |
| `appearance` | one of `system`, `light`, `dark` | `"system"` | Which of the two themes colours the console and the window: `system` follows macOS as it switches between light and dark, `light` and `dark` keep to one. |
| `dark_theme` | string | `"Nightfox"` | The theme drawn while the appearance is dark, named as the light one is. |
| `font_family` | string | `""` | The font the console and the prompt are drawn in, as the system names its family; empty for adroit's own, JetBrainsMono Nerd Font Mono. Adroit's own stands behind any other, so Nerd Font glyphs draw whatever font is chosen, and a Japanese font behind that. A line is as tall as the font asks, as in a terminal. |
| `font_size` | number | `13.0` | The font's size, in points, from 8 to 32. |
| `font_weight` | integer | `400` | The font's weight, from 100 to 900; adroit's own comes in regular and bold, and draws other weights in the nearer of them. |
| `keep_awake` | boolean | `true` | Keep the Mac from idle sleep while a session or a shell runs, the window open or not; the display still sleeps. |
| `light_theme` | string | `"Dawnfox"` | The theme drawn while the appearance is light, by name: one adroit comes with, or one in `themes/` beside `settings.toml`. |
| `notify_waiting` | boolean | `false` | Show a notification when a session starts waiting on you while the window is not in front; with several windows open, the one holding the lowest number tells. |
| `profile` | string | `"claude"` | The profile a new session starts from, by id, when it names neither a profile nor an agent; left out, the first profile not archived. A session not started yet can be switched to another agent for the current run of the window. |
| `prompt_max_rows` | integer | `12` | The most lines the prompt grows to before it scrolls inside. |
| `prompt_min_rows` | integer | `3` | The fewest lines the prompt shows. |
| `scratch_dir` | string | `""` | Where scratch sessions run, `~` standing for your home folder; empty for adroit's own folder. |
| `zoom` | number | `1.0` | How large the window draws everything when it opens, from 0.67 to 1.75. Command 0 comes back to it. |

## `[[connections]]`

The state directories profiles can run their agent's CLI in, each `[[connections]]` table one of them; every agent also runs in its own environment, which needs none.

| Setting | Type | Default | Description |
| --- | --- | --- | --- |
| `connections.agent` | one of `claude`, `codex` |  | The agent whose CLI keeps its state there, `claude` or `codex`. |
| `connections.archived` | boolean |  | Whether it is kept only for the profiles and sessions that used it. |
| `connections.home` | string |  | The state directory, as an absolute path without `~`, `.` or `..` and with no `/` doubled or at its end; it is fixed when the connection is made, since the CLIs tell logins apart by it as it is written. |
| `connections.id` | string |  | The connection's id, kept when it is renamed: lowercase letters, digits and `-`, and none of the agents' keys, which name their own environments. |
| `connections.name` | string | `""` | What the connection is called; empty for its id. |

## `[console]`

How the console takes keys and draws its cursor.

| Setting | Type | Default | Description |
| --- | --- | --- | --- |
| `console.cursor_blink` | boolean | `false` | Whether the cursor blinks when the program lets it. |
| `console.option_as_alt` | one of `both`, `left`, `right`, `none` | `"both"` | Which Option keys send Alt to what runs there, `both`, `left`, `right` or `none`, the others typing what macOS makes of the key. |

## `[delegation]`

How sessions started by sessions run.

| Setting | Type | Default | Description |
| --- | --- | --- | --- |
| `delegation.max_started` | integer | `4` | How many sessions started by sessions may run at once, from 0 to 64. Zero refuses new starts without ending those already running. Sessions you start and terminals do not count. |

## `[details]`

How much the details panel reads and shows.

| Setting | Type | Default | Description |
| --- | --- | --- | --- |
| `details.diff_lines` | integer | `2000` | The lines of a diff shown before it is cut. |
| `details.links_kept` | integer | `100` | The links of each kind the overview keeps, the newest, up to 1000. |
| `details.links_shown` | integer | `5` | The links of each kind the overview shows before a row that shows them all. |
| `details.log_commits` | integer | `50` | The changes Log lists. |

## `[neovim]`

How a Neovim tab's Neovim starts, as an agent does; an empty command runs `nvim`.

| Setting | Type | Default | Description |
| --- | --- | --- | --- |
| `neovim.command` | array of strings | `[]` | The program and the arguments it always gets, ahead of adroit's own; empty for the program it starts: the profile's agent's, or Neovim. |
| `neovim.env` | table of strings | `{}` | Variables set for it. |
| `neovim.via_shell` | boolean | `false` | Whether your shell starts it, as an interactive login shell, so that what its startup files set reaches it. |

## `[picker]`

Where the picker looks for directories to start a session in.

| Setting | Type | Default | Description |
| --- | --- | --- | --- |
| `picker.ghq` | boolean | `true` | Offer the repositories ghq keeps, and its roots in the file tree. |
| `picker.roots` | array of strings | `[]` | Folders whose folders are offered, and which the file tree lists, `~` standing for your home folder. |

## `[[profiles]]`

The profiles sessions start from, each `[[profiles]]` table one of them ([settings](../settings.md#profiles-and-connections)). An agent's default profile is the default profile when that starts the agent, else the agent's first profile not archived.

| Setting | Type | Default | Description |
| --- | --- | --- | --- |
| `profiles.agent` | one of `claude`, `codex` |  | The agent it starts, `claude` or `codex`. |
| `profiles.archived` | boolean |  | Whether it is kept only for the sessions that ran from it, and offered for no new one. |
| `profiles.connection` | string |  | The connection it runs in, by id; left out for the agent's own environment, which the agent's key also names. |
| `profiles.id` | string |  | The profile's id, kept when it is renamed: lowercase letters, digits and `-`. |
| `profiles.name` | string | `""` | What the profile is called; empty for its id. |

### `[profiles.claude]`

Claude Code's model, permission mode and tools denied, for a `claude` profile; what is left empty is Claude Code's own choice.

| Setting | Type | Default | Description |
| --- | --- | --- | --- |
| `profiles.claude.confirm_channel_warning` | boolean | `true` | Whether Adroit confirms, once a launch, Claude Code's warning about loading development channels when it names Adroit's own channel and nothing else; every other dialog is left to you. It applies to sessions launched while it is on. |
| `profiles.claude.disallowed_tools` | array of strings | `[]` | The tools denied it. |
| `profiles.claude.model` | string | `""` | The model it starts with, as Claude Code's `--model` takes it. |
| `profiles.claude.permission_mode` | string | `""` | Its permission mode, one of `default`, `acceptEdits`, `plan` and `bypassPermissions`. |

### `[profiles.codex]`

Codex's model and reasoning effort, approval policy and sandbox, for a `codex` profile; empty values leave Codex's own settings in effect.

| Setting | Type | Default | Description |
| --- | --- | --- | --- |
| `profiles.codex.approval_policy` | string | `""` | When Codex asks before running a command: empty, `on-request` or `never`. |
| `profiles.codex.model` | string | `""` | Codex's model; empty for its own choice. |
| `profiles.codex.reasoning_effort` | string | `""` | Its reasoning effort, one the selected model and Codex version support; empty for its own choice. |
| `profiles.codex.sandbox_mode` | string | `""` | Where its commands can write: empty, `read-only`, `workspace-write` or `danger-full-access`. |

### `[profiles.launch]`

How its agent is started; an empty command runs the agent's own program.

| Setting | Type | Default | Description |
| --- | --- | --- | --- |
| `profiles.launch.command` | array of strings | `[]` | The program and the arguments it always gets, ahead of adroit's own; empty for the program it starts: the profile's agent's, or Neovim. |
| `profiles.launch.env` | table of strings | `{}` | Variables set for it. |
| `profiles.launch.via_shell` | boolean | `false` | Whether your shell starts it, as an interactive login shell, so that what its startup files set reaches it. |

When the file holds none, it holds these, as adroit writes them:

```toml
[[profiles]]
id = "claude"
name = "Claude Code"
agent = "claude"

[profiles.launch]
command = []
via_shell = false

[profiles.launch.env]

[profiles.claude]
model = ""
permission_mode = ""
disallowed_tools = []
confirm_channel_warning = true

[[profiles]]
id = "codex"
name = "Codex"
agent = "codex"

[profiles.launch]
command = []
via_shell = false

[profiles.launch.env]

[profiles.codex]
model = ""
reasoning_effort = ""
approval_policy = ""
sandbox_mode = ""
```

## `[shell]`

How a shell tab's shell starts.

| Setting | Type | Default | Description |
| --- | --- | --- | --- |
| `shell.command` | array of strings | `[]` | The shell and its arguments; empty for your account's login shell, started as a login shell. |
| `shell.env` | table of strings | `{}` | Variables set for it. |

## `[usage]`

Machine resources shown in Usage across all tabs.

| Setting | Type | Default | Description |
| --- | --- | --- | --- |
| `usage.cpu` | boolean | `true` | Show the whole machine's CPU use. |
| `usage.gpu` | boolean | `true` | Show the GPU's use, when the system reports it. |
| `usage.memory` | boolean | `true` | Show the physical memory used, as a share of all of it. |
| `usage.storage` | boolean | `true` | Show the capacity used on the startup volume. |
