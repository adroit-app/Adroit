# Request ownership

The ownership model keeps an agent's assignment separate from the place it
works in. Independent leaders can use the same directory, and workers can
belong to one leader across different working directories and agent providers.
Changing a session's name does not change its assignment.

The workspace and ownership projections refer to the same sessions. A
session without a known assignment remains unassigned. If its assigned leader
is missing or handles a different request, it remains reachable without being
assigned to another leader.

A Standalone session has no controller assignment. Choosing Leader creates its
own Leader and run, including in a directory below the repository root. Separate
Leader starts at the same location remain independent requests. Switching views
does not change a session's assignment.

An independent Leader created from a human-authorized handoff has its own run
and ownership, even when it uses the same workspace as the requesting Leader.

The `session start` command refuses automatic managed ownership at repository
or native workspace roots and in directories without VCS; users start those
Leaders in the window. Its starts below a repository root remain unassigned
unless an explicit assignment is supplied. An explicitly
assigned Worker stays a Worker even when it starts at the root. Existing
sessions are not reclassified from their paths.

The assignment and launch permissions are recorded before the Agent program is
released. A startup failure cannot turn into a successful run, and ambiguous
startup remains available for reconciliation.
