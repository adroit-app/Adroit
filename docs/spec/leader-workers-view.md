# Leader / Workers view

Workspaces defaults to showing Standalone sessions. A toggle also shows Leader
and Worker sessions. Hiding sessions changes only the sidebar; switching views
preserves the console, open tabs, prompts, and drafts.
Opening a Worker enters its ordinary native Agent session, where the user can
interrupt it directly.
Workers remain reachable before their first conversation begins. Provider startup
dialogs that need a choice, including model migration confirmations, can be
answered in the Worker's console to resume work.

Leader / Workers groups requests by repository, then sessions by the controller's
current request ownership. Workers stay with their Leader's repository even
when their working directories are elsewhere.
Two Leaders at the same repository root remain independent. A matching name,
directory, or conversation ancestry does not establish membership. An old native
terminal cannot stand in for its replacement. Unassigned sessions and terminals
remain accessible; an unavailable controller is shown explicitly.

Completed Workers close automatically and leave session navigation. Their results
remain available in request details. Manually closed Leaders leave navigation
without a persistent restart action, and their Workers are stopped as well.

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
