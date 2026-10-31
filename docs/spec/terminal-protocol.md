# The terminal protocol's versions

The windows and the work host speak to the terminal host of their instance
([the hosts](sessions.md#the-hosts)) in felis's protocol, at the revision of
felis Adroit is built with, and keep tags of Adroit's own on each session in
it: what it runs and where, what it is called, which session opened it.
Together these are the terminal protocol. Its version is a whole number, raised only when
what it sends changes shape, so builds that share a version work with each
other's terminal host.

## What the terminal host says

felis's protocol has no room to say a version, so the terminal host says its
versions in a file beside its socket, the socket's path with the extension
`speaks`, written once it listens:

```text
pid 4242
build 0.3.0 (v0.3.0-12-gabc123)
speaks 1
```

`pid` is the process listening, `build` the build of Adroit it is, and
`speaks` the versions it speaks, separated by spaces. The file is replaced
whole, never written in place, so it is never read half written.

## Judging a terminal host

A terminal host works with this build when the process listening on the
socket is the one the file names and it speaks a version this build does.
One whose file names another process, or that wrote no file, as one from
before the protocol had versions, says no version, and works with no build.

A terminal host that does not work with this build is never replaced on its
own: replacing it ends every session and terminal it holds. The window says
so, with the build the host says it is, and offers to restart it; it is
replaced only once the user confirms, or by `adroit daemon restart`.
