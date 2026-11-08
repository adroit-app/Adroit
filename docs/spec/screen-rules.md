# Screen rules

When an agent's program reports no status, Adroit reads how the session
stands from its screen ([how a session stands](sessions.md#how-a-session-stands)):
its state, whether a dialog of the agent's own holds the keys
([keys](keys.md)), and, while it starts, whether it is ready or asks
something first. It reads the screen by a set of rules for that agent.

Adroit carries a set for every agent it starts. Its own sets, in Adroit's
source under `crates/agents/rules/`, are a starting point for one of yours.

## A set of your own

A file `screen-rules/<agent>.toml` in Adroit's configuration folder —
`$XDG_CONFIG_HOME/adroit/`, or `~/.config/adroit/` — on the machine where the
sessions run takes the place of the whole set Adroit carries for `<agent>`:
`claude`, `codex`, `pi` or `opencode`. It is read again once it changes, is
made or is removed; nothing needs restarting.

A file that cannot be used is set aside, and the set Adroit carries reads the
screen instead: one that is not written as below, is written for another
agent, has a pattern that is no regular expression, or is larger than the
limits. `adroit session explain` names the set in force, and when your file
was set aside, says so and why.

Adroit fetches no rules from anywhere.

## The format

A set is a TOML file.

```toml
agent = "codex"

[lines]
chevron = '^\s*›'
interrupt = '^\s*• .*esc to interrupt'

[regions]
composer = { last = "chevron" }
above = { before = "composer" }
footer = { within = "above", since = "chevron" }

[[rules]]
id = "no_composer"
default = true
when = { absent = "composer" }

[[rules]]
id = "interrupt_hint"
state = "working"
when = { in = "footer", has = "interrupt" }

[[rules]]
id = "composer_idle"
state = "idle"
default = true
shows = "composer"
```

`agent` names the agent the set is for.

### Lines

`[lines]` names the kinds of row the rules look for. A line is a regular
expression, in the syntax of Rust's `regex` crate, that a row matches when the
expression matches anywhere in it; `(?i)` makes it ignore case. Written as a
table, `{ regex = '…', next = "<line>" }`, a row also needs the row right
below it to be that other line.

### Regions

`[regions]` names stretches of the screen's rows that the rules look in. A
region is found on the whole screen, or, with `within = "<region>"`, inside
another region; it is found nowhere when the region it is within is found
nowhere. It chooses its rows by exactly one of:

| Key | Rows |
| --- | --- |
| `last = "<line>"` | the last row that is that line; nowhere when no row is |
| `since = "<line>"` | the rows after the last row that is that line, or all the rows when no row is |
| `after = "<region>"` | the rows after that region; nowhere when it is |
| `before = "<region>"` | the rows before that region; nowhere when it is |
| `from = "<region>"` | the rows from that region's first to the end; nowhere when it is |
| `head = N` | the first N rows |
| `tail = N` | the last N rows |
| `tail_non_empty = N` | the rows from the Nth last that shows something to the end |

A region may not be built on itself, through other regions or directly.

### Conditions

A condition is a table whose every key must hold:

| Key | Holds when |
| --- | --- |
| `has = "<line>"` | some row is that line; with `in = "<region>"`, some row of that region, which then must be found |
| `present = "<region>"` | the region is found |
| `absent = "<region>"` | the region is found nowhere |
| `holds = "<condition>"` | the condition named in `[conditions]` holds |
| `all = [ … ]` | every condition in the list holds |
| `any = [ … ]` | at least one condition in the list holds |
| `not = { … }` | that condition does not hold |

`[conditions]` names conditions that rules use by `holds`. A named condition
may not use `holds` itself.

### Rules

`[[rules]]` are tried in order, and the first whose `when` holds decides. A
rule without `when` holds on any screen, and the last rule has none.

| Key | Means |
| --- | --- |
| `id` | the rule's name; `adroit session explain` names it `<agent>.<id>` |
| `when` | the condition the rule holds on |
| `state` | `working`, `idle`, `waiting-input` or `waiting-permission`; without it the screen tells no state |
| `dialog` | `true`, or a condition: whether a dialog of the agent's own holds the keys while this rule decides |
| `startup` | `ready`, `awaiting-trust`, `awaiting-permission` or `awaiting-input`: how a session still starting stands, in place of what its state says |
| `default` | `true` when the rule decides for want of a sign: a working session that this rule reads idle is held working until the screen is read idle once more |
| `shows` | a region whose rows `adroit session explain` shows as those the rule went by, beside the rows its `has` conditions matched |

A session still starting stands as its rule's `startup` says, or else as its
state says: `idle` and `working` are ready, `waiting-input` awaits input and
`waiting-permission` awaits permission. A set may instead tell how a session
starts by rules of their own, `[[startup]]`, tried the same way in place of
`[[rules]]`; each takes `id`, `when`, `startup`, `default` and `shows`, and
without `startup` tells that nothing is shown yet.

`id`s are unique within each list.

### Limits

A set is refused when a list holds more than 64 rules, a table names more than
64 lines, regions or conditions, a pattern is longer than 512 characters, or
conditions nest more than 8 deep.
