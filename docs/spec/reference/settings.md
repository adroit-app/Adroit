# Settings reference

<!-- Written by `just references` from the code; edit the code, not this file. -->

Every setting `settings.toml` holds, table by table: its type, what it is
when the file does not name it, and what it does
([settings](../settings.md)).

## The top level

| Setting | Type | Default | Description |
| --- | --- | --- | --- |
| `agent` | one of `claude`, `codex` | `"claude"` | The agent a new session starts with, `claude` or `codex`; a session not started yet can be switched to another for the current run of the window. |
| `appearance` | one of `system`, `light`, `dark` | `"system"` | Which of the two themes colours the console and the window: `system` follows macOS as it switches between light and dark, `light` and `dark` keep to one. |
| `dark_theme` | string | `"Nightfox"` | The theme drawn while the appearance is dark, named as the light one is. |
| `font_family` | string | `""` | The font the console and the prompt are drawn in, as the system names its family; empty for adroit's own, JetBrainsMono Nerd Font Mono. Adroit's own stands behind any other, so Nerd Font glyphs draw whatever font is chosen, and a Japanese font behind that. A line is as tall as the font asks, as in a terminal. |
| `font_size` | number | `13.0` | The font's size, in points, from 8 to 32. |
| `font_weight` | integer | `400` | The font's weight, from 100 to 900; adroit's own comes in regular and bold, and draws other weights in the nearer of them. |
| `keep_awake` | boolean | `true` | Keep the Mac from idle sleep while a session or a shell runs, the window open or not; the display still sleeps. |
| `light_theme` | string | `"Dawnfox"` | The theme drawn while the appearance is light, by name: one adroit comes with, or one in `themes/` beside `settings.toml`. |
| `notify_waiting` | boolean | `false` | Show a notification when a session starts waiting on you while the window is not in front; with several windows open, the one holding the lowest number tells. |
| `prompt_max_rows` | integer | `12` | The most lines the prompt grows to before it scrolls inside. |
| `prompt_min_rows` | integer | `3` | The fewest lines the prompt shows. |
| `scratch_dir` | string | `""` | Where scratch sessions run, `~` standing for your home folder; empty for adroit's own folder. |
| `zoom` | number | `1.0` | How large the window draws everything when it opens, from 0.67 to 1.75. Command 0 comes back to it. |

## `[claude]`

How Claude Code starts for every session adroit runs; an empty command runs `claude`.

| Setting | Type | Default | Description |
| --- | --- | --- | --- |
| `claude.command` | array of strings | `[]` | The program and the arguments it always gets, ahead of adroit's own; empty for the program its table names. |
| `claude.env` | table of strings | `{}` | Variables set for it. |
| `claude.via_shell` | boolean | `false` | Whether your shell starts it, as an interactive login shell, so that what its startup files set reaches it. |

## `[codex]`

How Codex starts, independently of Claude Code; an empty command runs `codex`.

| Setting | Type | Default | Description |
| --- | --- | --- | --- |
| `codex.command` | array of strings | `[]` | The program and the arguments it always gets, ahead of adroit's own; empty for the program its table names. |
| `codex.env` | table of strings | `{}` | Variables set for it. |
| `codex.via_shell` | boolean | `false` | Whether your shell starts it, as an interactive login shell, so that what its startup files set reaches it. |

## `[codex_sessions]`

Codex's model and reasoning effort, approval policy and sandbox; empty values leave Codex's own settings in effect.

| Setting | Type | Default | Description |
| --- | --- | --- | --- |
| `codex_sessions.approval_policy` | string | `""` | When Codex asks before running a command: empty, `on-request` or `never`. |
| `codex_sessions.model` | string | `""` | Codex's model; empty for its own choice. |
| `codex_sessions.reasoning_effort` | string | `""` | Its reasoning effort, one the selected model and Codex version support; empty for its own choice. |
| `codex_sessions.sandbox_mode` | string | `""` | Where its commands can write: empty, `read-only`, `workspace-write` or `danger-full-access`. |

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

How a Neovim tab's Neovim starts, as Claude Code does; an empty command runs `nvim`.

| Setting | Type | Default | Description |
| --- | --- | --- | --- |
| `neovim.command` | array of strings | `[]` | The program and the arguments it always gets, ahead of adroit's own; empty for the program its table names. |
| `neovim.env` | table of strings | `{}` | Variables set for it. |
| `neovim.via_shell` | boolean | `false` | Whether your shell starts it, as an interactive login shell, so that what its startup files set reaches it. |

## `[picker]`

Where the picker looks for directories to start a session in.

| Setting | Type | Default | Description |
| --- | --- | --- | --- |
| `picker.ghq` | boolean | `true` | Offer the repositories ghq keeps, and its roots in the file tree. |
| `picker.roots` | array of strings | `[]` | Folders whose folders are offered, and which the file tree lists, `~` standing for your home folder. |

## `[sessions]`

How Claude Code sessions run; what is left empty is Claude Code's own choice.

| Setting | Type | Default | Description |
| --- | --- | --- | --- |
| `sessions.confirm_channel_warning` | boolean | `true` | Whether Adroit confirms, once a launch, Claude Code's warning about loading development channels when it names Adroit's own channel and nothing else; every other dialog is left to you. It applies to sessions launched while it is on. |
| `sessions.disallowed_tools` | array of strings | `[]` | The tools denied it. |
| `sessions.model` | string | `""` | The model it starts with, as Claude Code's `--model` takes it. |
| `sessions.permission_mode` | string | `""` | Its permission mode, one of `default`, `acceptEdits`, `plan` and `bypassPermissions`. |

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
