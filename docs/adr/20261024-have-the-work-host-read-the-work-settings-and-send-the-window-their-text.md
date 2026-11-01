# Have the work host read the work settings and send the window their text

Status: Accepted

## Context

The work settings are a file the user edits by hand and the Settings window
writes. The work host acts on them: how agents start, what the picker offers,
keeping the Mac awake. The window acts on them too, but reaches the work
machine only through the work host, and may run on another machine.

## Decision

The work host reads the work settings file, again whenever it changes, and
sends each window its text as part of the work. The window reads the work
settings from that text. The Settings window changes them by asking the work
host to write the file.

## Alternatives considered

- Each window reads the file itself. A window on another machine cannot, and
  windows reading at different moments disagree while the file is being
  edited.
- The work host sends the settings already read. Every new setting would
  change what the protocol sends, and a mistake in the file would be reported
  in the host's terms rather than where the user sees it.

## Consequences

- Adding a setting changes nothing the protocol sends.
- A window and the work host read the same text, of the same build, the same
  way.
- A mistake in the file is reported in every window, and the settings in force
  stay as they were until it is fixed.
