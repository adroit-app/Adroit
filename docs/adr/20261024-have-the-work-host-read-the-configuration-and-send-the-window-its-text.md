# Have the work host read the configuration and send the window its text

Status: Accepted

## Context

The settings and the key bindings are files the user edits by hand and the
Settings window writes. The work host acts on the settings: how agents start,
what the picker offers, keeping the Mac awake. The window acts on them too, and
on every key binding, but reaches the hosts' machine only through the work
host, and may in time run on another machine.

## Decision

The work host reads the settings and key bindings files, again whenever they
change, and sends each window their text as part of the work. The window
reads its settings and bindings from that text. The Settings window changes
them by asking the work host to write the files.

## Alternatives considered

- Each window reads the files itself. A window on another machine cannot, and
  windows reading at different moments disagree while a file is being edited.
- The work host sends the settings already read. Every new setting would
  change what the protocol sends, and a mistake in a file would be reported
  in the host's terms rather than where the user sees it.

## Consequences

- Adding a setting or an action changes nothing the protocol sends.
- A window and the work host read the same text, of the same build, the same
  way.
- A mistake in a file is reported in every window, and the bindings and
  settings in force stay as they were until it is fixed.
