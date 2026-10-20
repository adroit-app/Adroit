# Native control tools

Managed Agents receive the same scoped control API through native MCP tools.
The published input schema describes the command variants and their nested run,
task, result, permissions, candidate, and coordination formats. Human-only
management operations are absent from that schema and remain refused to Agent
credentials even if supplied manually.

`control` takes `request_id` and `command`. The command is an object tagged with
`op`. The session credential supplies the actor; callers cannot choose another
identity. A request ID identifies an immutable command, including retries after
a lost response. Changed content needs a new request ID.

Tool text results contain JSON with either `Ok` or `Err`. Successful mutations
return their record in the reply's `value`; reads return their documented value.
Errors contain `code` and `message`. Errors do not authorize a fallback through
another identity or an untracked session launch.

`new_ids` allocates fresh UUIDs without changing orchestration state. Its optional
integer counts are `requests`, `tasks`, `messages`, `candidates`, `runs`, and
`coordination`. Requests default to eight; other kinds default to zero. Counts
range from zero to 32, with at most 64 IDs in one call. The result contains the
corresponding `request_ids`, `task_ids`, `message_ids`, `candidate_ids`, `run_ids`,
and `coordination_ids` arrays.

`fingerprint_files` takes a `paths` array. It identifies declared file outputs in
the current Worker's assigned working area and returns their canonical fingerprint
without file contents. It refuses Leaders and obsolete assignments.

A Leader reads `context`, updates its existing run, records tasks with `put_task`,
and makes them eligible with `queue_task`. It finishes its turn after dispatch;
notifications bring back results and questions. A Worker uses `send_message` for
progress and questions and `report_task` for a final structured outcome. Neither
session names nor matching directories establish ownership.
