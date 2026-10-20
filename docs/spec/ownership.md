# Request ownership

The ownership model keeps an agent's assignment separate from the place it
works in. Independent leaders can use the same directory, and workers can
belong to one leader across different working directories and agent providers.
Changing a session's name does not change its assignment.

The workspace and ownership projections refer to the same sessions. A
session without a known assignment remains unassigned. If its assigned leader
is missing or handles a different request, it remains reachable without being
assigned to another leader.
