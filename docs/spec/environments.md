# Environments

An environment is a named place to work: its connection, its agents and its
work settings. The main window's tree lists every environment kept on this
computer, each under a heading of its own ([tree](tree.md)), and the window
connects to all of them when it opens and to each one added while it is open.
An environment that cannot be reached stays listed with the work last received
from it, the window keeps trying it, and the window's other environments stay
usable. Each one's tabs come back once its host lists its work. The header
names the environment of the tab shown. Settings configures the environment of
the selected tree row or the tab shown. Environments are created, their
connections set up and removed ones restored in the separate Environments
window, opened from the application menu; opening an environment there goes to
its heading. An environment can also be opened in a separate, independent
window instance of its own.

Removing an environment from its heading, after asking, archives it and takes
it out of every window's tree; its sessions keep running on its host. A window
cannot remove the environment it was started in.

Each instance keeps the environment and connection it opened. Changing another
instance's choice does not redirect its CLI, MCP, drafts or unresolved sends.
Renaming an environment keeps its identity. Choosing a different connection in
the environment editor creates another environment, leaving the original alone.

The agent profiles a session starts from ([settings](settings.md#profiles))
live inside an environment; an environment is not one of them.

## Create and configure

An environment uses this computer or a work machine this computer paired with
([paired work machines](remote-execution.md#paired-work-machines)). The
Environments window can pair with a work machine: it lists the work machines
heard on the local network or takes an address, shows the code to type on the
work machine, and once that is done, saves an environment for the work
machine, named after it unless a new environment's name was typed. Pairing
again with a machine that already has an environment selects that
environment. Pairing from the command line adds an environment of its own too.
An environment can switch between its paired work machine and the other
connections. Connection testing does not select or create an environment. An
environment can be saved before its work machine or agents are available.

Open an environment to configure its agents, working directories and programs.
Its agent profiles use the programs installed on that work machine, and
installation and login checks run there. They never substitute the client's
agents or credentials. Several agent profiles, including different agents,
can coexist in an environment. Agent launch changes apply to new sessions, not
programs already running.

Environments sharing a work machine use the same host services but keep
separate work settings and session lists. This is an organizational boundary,
not an OS user or permission boundary. An agent using the work user's normal
login still shares that login; an agent's dedicated login remains on the work
machine. Stale settings saves are refused so an older window cannot overwrite
a newer configuration.

Fonts, zoom, prompt dimensions, notifications and local console input
preferences are saved on this computer across environments. The embedded
Neovim configuration also stays here. Work-machine paths and console themes
remain part of the environment. When disconnected, the Environments window and
local preferences remain available; work settings cannot be saved locally in
their place.

## Retain and recover work

Closing a window or removing an environment's registration does not stop its
sessions or remove its credentials and files. Removed registrations remain
restorable from the Environments window. Creating another environment with the
same connection does not copy credentials, dedicated login directories or
running work.

Reopening an environment returns to its own sessions and settings. A
replacement work host preserves the terminal host and its sessions. Uncertain
sends remain bound to their original target and are queried there, never
replayed in another environment. Remote console input and interactive login
follow the rules of [remote hosts](remote-execution.md).

## Existing installations

An existing instance is registered as an environment of its own. Agent
profile IDs, login paths, sessions and history are retained. A saved but
unapplied connection is imported as another unopened choice; importing does
not silently apply it. The agent profiles' `[[profiles]]` format in
`settings.toml` and the CLI option names remain compatible.

The client catalog is `environments.toml` beside the client's existing
`settings.toml`. It has `version = 1`, a monotonically increasing `revision`,
and `[[environments]]` entries:

- `id`: a stable instance-safe identifier, independent of its display name.
- `name`: a nonempty display name, at most 256 bytes.
- `target`: a table in the [execution target format](remote-execution.md#execution-configuration).
- `scope`: the work environment identifier; omitted for a migrated legacy
  environment. It follows the instance-name rules.
- `archived`: whether the registration was removed, defaulting to false.
- `legacy_client`: the original client instance, when imported from an existing
  installation; otherwise omitted.
- `work_instance`: the original local service instance for an imported saved
  connection; otherwise omitted.

A catalog that earlier versions wrote with `[[profiles]]` entries is read as
if they were `[[environments]]`, and written with `[[environments]]` the next
time it is saved; one that holds both is refused.

The catalog revision is checked before changes are saved. Unknown versions,
invalid entries and duplicate IDs are refused. Each instance pins the
environment it opened, so editing the catalog never replaces the connection or
the environment of an instance already open. `window.toml` beside the catalog
holds only the local display and input preferences. Missing local preferences
inherit those fields from the existing client settings, without taking agent
settings from the work machine.
