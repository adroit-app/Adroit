# Native control tools

Managed Agents receive the same scoped control API through native MCP tools.
Their launch permits Adroit's three scoped MCP methods without a separate approval
for each internal request. This does not grant permission to unrelated MCP tools,
shell commands, or filesystem operations.
The published input schema describes the command variants and their nested run,
task, result, permissions, candidate, and coordination formats. Human-only
management operations are absent from that schema and remain refused to Agent
credentials even if supplied manually.

Before the first `control` call, allocate request IDs with `new_ids`.
`control` takes a UUID `request_id` and `command`. The command is an object tagged with
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

## Large native notifications

Small notifications contain the complete message. Larger serialized messages
use a bounded reference while the complete immutable body remains stored. The
reference has `type` equal to `adroit_message_reference`, `version` equal to 1,
`message_id`, `run_id`, `from`, `to`, nullable `task_id`, `attempt_id`, and
`revision`, and the message `kind`. It contains no body and is not a report by
itself.

`read_message` takes `message_id` and returns the complete original message.
Only its sender or recipient in the assigned run can fetch it through a session
credential. Acknowledgment does not remove the stored body, and fetching it does
not acknowledge or finish work. This reference format is shared by the native
Codex queue and Claude channel routes; neither route types into the console.
