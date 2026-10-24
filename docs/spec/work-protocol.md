# The work protocol

What the windows and the `adroit` commands say to the work host of their
instance ([the hosts](sessions.md#the-hosts)), on its socket. Each message is
one JSON object on a line of its own.

## Hello

On every connection, before it is asked anything, the work host says which
versions of the protocol it speaks and which build of adroit it is:

```json
{"speaks":[1],"build":"0.3.0 (v0.3.0-12-gabc123)"}
```

This line has the same shape in every version, so any build can judge any
work host by it. A version is a whole number, raised only when what the
protocol sends changes shape; the two sides work in the newest version both
speak.

A window keeps a work host of its own build that shares a version with it, and
replaces any other, which ends no session; where none answers, it starts one.
A command asks the work host it finds, of another build too, while they share
a version, and starts one where none answers. One that shares no version is
refused with a word to the user.

`{"op":"stop"}` asks the work host to stop, in every version: it answers
`{"stopping":true}` and goes. `adroit host stop` and `adroit host restart`
ask so.

## The work, as a window shows it

A window says hello in turn, naming the versions it speaks. The work host
welcomes it in the version they share, or refuses it, naming the versions it
speaks.

Welcomed, the window says what it shows, again whenever that changes: the
directories it has unfolded, the session its details describe, whether its
picker is open. The host sends it the work as it shows it, whole, as a
snapshot, and from then on every change to that as a patch. Each patch is
numbered one past the one before it, and a snapshot carries the number of
the last change it holds, so a window applies each patch on top of the one
before it. A window that finds a number missing asks for the whole work
again, and takes what follows the new snapshot.

Each window connected is told only of what it shows. A change one window
makes, such as a session started, ended or renamed, reaches every window that
shows it.

## Requests and asks

What a window does to the work, it sends as a request, and the host's change
comes back as a patch. What a window or a command waits for, starting a
session or a terminal, saving the settings, keeping a pasted image, running
a command, is an ask: the asker numbers it, and the host answers it once, by
that number, to the asker alone, with what came of it or why it failed. A
notice or a failure the host has for the user goes to the window whose request
it concerns.

## When the work host goes

A window that loses its work host connects again, starting one where none
answers, says hello, says what it shows, and is sent the whole work afresh.
An ask it sent and that was not answered fails, with a word to the user: it
is not known to be done. The sessions' tools, which reach the work host
apart from this protocol, wait a few seconds for a new one before they fail.
Nothing of the work goes with a work host: what it keeps between runs is on
disk, the rest it reads again, and the sessions run on in the terminal host.
