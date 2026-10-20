# Request ownership

The ownership model keeps an agent's assignment separate from the place it
works in. Independent leaders can use the same directory, and workers can
belong to one leader across different working directories and agent providers.
Changing a session's name does not change its assignment.

The workspace and ownership projections refer to the same sessions. A
session without a known assignment remains unassigned. If its assigned leader
is missing or handles a different request, it remains reachable without being
assigned to another leader.

A new human session at a repository or native workspace root automatically
receives its own Leader and run. A selected directory without VCS receives the
same treatment. Separate starts at the same root remain independent requests.
A start below a repository root remains unassigned unless an explicit task
assignment is supplied. An explicitly assigned Worker stays a Worker even when
it starts at the root. Existing sessions are not reclassified from their paths.

The assignment and launch permissions are recorded before the Agent program is
released. A startup failure cannot turn into a successful run, and ambiguous
startup remains available for reconciliation.
