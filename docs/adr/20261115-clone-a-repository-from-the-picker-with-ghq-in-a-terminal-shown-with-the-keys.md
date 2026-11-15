# Clone a repository from the picker with ghq in a terminal shown with the keys

Status: Accepted

## Context

The picker offers the repositories ghq keeps to start a session in, but a
repository not cloned yet has to be cloned outside Adroit first. Cloning asks
the network and often the user: an SSH key's passphrase, a host key to trust,
a login for HTTPS. It can take from a moment to minutes, and it runs on the
work machine, which may not be the computer the window is on.

## Decision

A repository named in the picker's query is offered to clone. Choosing it has
the work host of the environment it is offered in run `ghq get` in a terminal
the window shows with the keys, as it shows any terminal, and kept once the
command ends. Once it ends having cloned the repository, the window does with
the repository's directory what choosing that directory would have done.

## Alternatives considered

- Clone in the work host without a terminal and report how it went. Whatever
  the clone asks has nowhere to be answered, so cloning over SSH with a key
  that has a passphrase, or from a host not yet trusted, fails.
- Clone with git into a folder Adroit picks. The user already keeps their
  repositories where ghq puts them, and ghq's own settings say where each
  host's go; a second layout would split them.
- Leave cloning to a shell the user opens. It is what this replaces: going
  out of the picker for every repository not cloned yet.

## Consequences

- Cloning is offered only where ghq is installed and the picker is let to ask
  it.
- What the clone writes and asks stays readable in its terminal, failed or
  not, until the user closes it.
- A work machine reached from another computer clones with its own
  credentials, as its sessions run with them.
