# Environment profiles

A profile names a working environment: its connection, agents and work settings.
The main window's profile menu opens a saved profile or a separate profile
management window. Profile creation and connection setup are available there;
Settings configures the environment already open in the current window.
Opening a profile brings its window forward or opens one; the previous window
and its running work remain available. A profile can also be opened in a
separate, independent window instance.

Each instance keeps the environment and connection it opened. Changing another
instance's choice does not redirect its CLI, MCP, drafts or unresolved sends.
Renaming a profile keeps its identity. Choosing a different connection in the
profile editor creates another profile, leaving the original environment alone.

## Create and configure

Profiles can use this computer or a work machine this computer paired with
([paired work machines](remote-execution.md#paired-work-machines)).
Profile management can pair with a work machine: it lists the work machines
heard on the local network or takes an address, shows the code to type on the
work machine, and once that is done, saves a profile for the work machine,
named after it unless a new profile's name was typed. Pairing again with a
machine that already has a profile selects that profile. Pairing from the
command line adds a profile of its own too. A profile can switch between its paired work machine and the other
connections. Connection testing does
not select or create a work environment.
A profile can be saved before its work machine or agents are available.

Open a profile to configure its Agents, working directories and programs. Agent
setups use the programs installed on that work machine, and installation and
login checks run there. They never substitute the client's agents or credentials.
Several agent setups, including different agents, can coexist in a profile.
Agent launch changes apply to new sessions, not programs already running.

Profiles sharing a work machine use the same host services but keep separate
work settings and session lists. This is an organizational boundary, not an OS
user or permission boundary. An agent using the work user's normal login still
shares that login; an agent's dedicated login remains on the work machine.
Stale settings saves are refused so an older window cannot overwrite a newer
configuration.

Fonts, zoom, prompt dimensions, notifications and local console input preferences
are saved on this computer across profiles. The embedded Neovim configuration
also stays here. Work-machine paths and console themes remain part of the work
environment. When disconnected, profile management and local preferences remain
available; work settings cannot be saved locally in their place.

## Retain and recover work

Closing a window or removing a profile's registration does not stop its sessions
or remove its credentials and files. Removed registrations remain restorable
from profile management. Creating another profile with the same connection does
not copy credentials, dedicated login directories or running work.

Reopening an environment returns to its own sessions and settings. A replacement
work host preserves the terminal host and its sessions. Uncertain sends remain
bound to their original target and are queried there, never replayed in another
profile. Remote console input and interactive login follow the rules of
[remote hosts](remote-execution.md).

## Existing installations

An existing instance is registered as a profile of its current environment.
Agent setup IDs, login paths, sessions and history are retained. A saved but
unapplied connection is imported as another unopened choice; importing profiles
does not silently apply it. The existing `[[profiles]]` agent-setup format and
CLI option names remain compatible.

The client catalog is `environments.toml` beside the client's existing
`settings.toml`. It has `version = 1`, a monotonically increasing `revision`,
and `[[profiles]]` entries:

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

The catalog revision is checked before changes are saved. Unknown versions,
invalid entries and duplicate IDs are refused. Each instance pins the profile
it opened, so editing the catalog never replaces the connection or the
environment of an instance already open. `window.toml` beside the catalog
holds only the local display and input preferences. Missing local preferences inherit those fields from the existing
client settings, without taking agent settings from the work machine.
