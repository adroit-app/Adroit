# Give an OpenCode account of its own only the variables it is known to need

Status: Accepted

## Context

A connection of its own keeps an agent to its own login: adroit removes the
variables known to carry another login before the agent's program starts.
OpenCode connects every provider whose credential variable models.dev lists,
a list it fetches as it runs and that grows with every release, and that
names variables other programs read as well, such as a forge's token. A list
of what to remove would always lag behind what OpenCode reads.

## Decision

In a connection of its own, OpenCode gets only the variables of a short list
of those that locate the user, the terminal and the network, adroit's own,
and those its profile's launch sets.

## Alternatives considered

- Remove the variables known to carry a login, as for the other agents. A
  provider added to models.dev, or a variable OpenCode reads that adroit has
  not listed, would bring another login into the account unseen.
- Start OpenCode with no variables but the account's. It could not find the
  programs it runs, the user's terminal or the network.

## Consequences

- No variable of the environment adroit runs in reaches an OpenCode account
  unless it is on the list; a profile sets what its sessions need beyond it,
  as a key or a toolchain's folder, in its launch.
- The commands an OpenCode session runs get the same short environment, the
  folders the account moved put back.
- The deny-list agents keep their environment as it was.
