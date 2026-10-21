# Leader / Workers view

Workspaces and Leader / Workers are views of the same Agent and Terminal
sessions. Switching views preserves the console, open tabs, prompts, and drafts.
Opening a Worker enters its ordinary native Agent session, where the user can
interrupt it directly.

Leader / Workers groups sessions by the controller's current request ownership.
Two Leaders at the same repository root remain independent. A matching name,
directory, or conversation ancestry does not establish membership. An old native
terminal cannot stand in for its replacement. Unassigned sessions and terminals
remain accessible; an unavailable controller is shown explicitly.

Request details show the goal, task progress, current result summaries,
verification commands, review decisions, unanswered questions, and shared default
workspace changes. Requested, selected, and reported actual models are distinct;
an unreported actual model stays unknown. Task details can discover earlier
attempts and their reports, including after an attempt is replaced or cleared.
Earlier revisions are marked historical, and full report bodies load on request.
Related Leader and Worker sessions can
be opened from their details when their native terminals are available.

Shell and Neovim remain Terminal resources. An explicit request association may
place them with a Leader; they never become Workers or consume Worker slots.
