# Run on Windows with named pipes and stand-ins for the Unix calls

Status: Accepted

## Context

Adroit was written for Unix: its hosts listen on Unix sockets in private
folders, keep files private by their mode bits and owner, start a detached
host through a shell that leads a session of its own, and hold a session's
program back behind `/bin/sh` until it may run. Windows has none of these.
felis, which holds the terminals, already runs there: it speaks over named
pipes it closes to other users, and gives each session a ConPTY.

## Decision

The window, the work host and the terminal host run on Windows as they do on
Unix, each process the same, and the code paths stay one where they can.

- A socket keeps its path in the instance's folder, where the files kept
  beside it stay; only where it is listened at or reached does that path name
  a named pipe on Windows.
- The Unix calls Adroit makes on files, processes and sockets are made through
  one crate that is the system's own on Unix and stands in for them on
  Windows. A file there is kept private by the access list of the profile it
  is in, so the stand-ins set no mode and pass the checks of owner and mode.
- What Windows cannot do the Unix way is done another way there: Adroit holds
  a session's program back itself, and starts a program directly, as no
  Windows shell sets up the environment it is meant to have.
- What has no Windows counterpart is left out there: the startup lease, which
  replaces itself with the program it holds, and the window's socket, by which
  one window is brought to the front and offered an external editor's input.

## Alternatives considered

- A client only, the window reaching a work machine elsewhere. Windows would
  run no session of its own, though felis already holds terminals there.
- A branch for each platform at every Unix call. The checks that keep files
  private would be written twice and could drift apart.
- Loopback TCP in place of named pipes. Any process of any user could connect,
  and felis speaks pipes already.
- Run the hosts under WSL. The sessions would see Linux, not the Windows the
  user works in.

## Consequences

- Modes, owners and links are checked on Unix only; on Windows, keeping files
  private rests on the profile's access list.
- Adroit's own process stays the parent of every session's program on
  Windows, between the terminal host and the program.
- On Windows a held program is let go only through its terminal, never by a
  lease of its own, so one whose work host is gone before it is let go waits.
- On Windows each `adroit` opens a window of its own, and Claude Code's
  external editor is not handed to the window.
- The tests start programs and sockets only Unix has, and run on Unix alone;
  Windows is checked by building and linting there.
