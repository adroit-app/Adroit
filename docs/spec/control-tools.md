# Native control tools

Managed Agents receive the same scoped control API through native MCP tools.
Their launch permits Adroit's scoped MCP methods without a separate approval
for each internal request. This does not grant permission to unrelated MCP tools,
shell commands, or filesystem operations.
Each operation is a named tool with a generated input schema containing its
fields and reachable types. Human-only management and internal delivery claims
are absent from the catalog. Human management remains refused to Agent
credentials even if supplied manually through the control protocol.

A user may authorize an independent request in the Adroit window for one managed
Leader by specifying its workspace, request brief, agent, model, and permissions. Only
that Leader can use the returned authorization with `create_independent_run`.
The operation returns new run and Leader IDs and starts a fresh managed Leader
with the authorized request. A Worker cannot redeem the authorization through
the control API. The socket gives the local human credential read-only access;
lifecycle changes come from app actions or scoped managed sessions. The CLI
control bridge requires a managed session credential. Repeating the
same request ID returns the same IDs; a new request cannot redeem an already
used authorization. Missing, linked, or replaced workspaces are checked at
authorization, redemption, and immediately before launch. A pathname can still
change between that check and native use. Same-user arbitrary code is outside
this interaction boundary.

Read `context` first. The credential supplies the actor; callers cannot choose
another identity. `request_id` is optional. The bridge generates it when omitted
and returns it with mutation replies. Preserve it for an identical retry after a
lost response; changed content needs a new request ID. Explicit UUID request IDs
remain supported.

Native arguments use the control protocol's operation fields without `op` or
`command`. The following single record wrappers are also flattened:

| Native tool | Protocol wrapper whose fields become tool arguments |
| --- | --- |
| `put_task` | `plan` |
| `queue_task` | `version` |
| `report_task` | `report` |
| `send_message` | `message` |
| `propose_candidate` | `plan` |
| `announce_change` | `intent` |

For example, `put_task` takes `request_id`, `id`, `run_id`, `expected_revision`,
`brief` and `dependencies`. A new task uses `expected_revision: null`.
`queue_task` takes `request_id`, `task_id` and `revision`. Other operation fields
retain the formats defined in the control protocol. The CLI continues to take
its tagged command object; native tool arguments do not use that wrapper.

Tool text returns compact JSON for reads and `ok` with identity, revision and
relevant state for successful mutations. Errors return `code: message` and do not
authorize another identity or an untracked session launch. The CLI and socket
retain their complete JSON records.

Native tools accept full UUIDs or short aliases issued to the calling logical
session. Aliases survive bridge and daemon restarts and Leader generations;
another session's aliases cannot be transferred. Prefixes are `w` for sessions,
`r` for runs, `t` for tasks, `e` for attempts, `m` for messages, `a` for working
areas, `c` for candidates, `n` for coordination notices, and `h` for immutable
revisions. Aliases do not grant access: ordinary ownership checks still apply.
User prose and arbitrary JSON evidence are preserved. `resolve_ids` takes an
`ids` array and returns full identities for cross-session handoffs. Mailbox and
snapshot reads show report summaries with detail references; `read_message`
provides complete evidence.

When a running daemon predates native projections, the bridge uses full UUIDs
and ordinary authenticated control operations. It leaves existing sessions
running; short aliases require a daemon that supports them.

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
questions and material changes and `report_task` for a final structured outcome. Neither
session names nor matching directories establish ownership.

## Native notifications

Routine Worker progress notes remain in the mailbox and UI without waking the
Leader. Questions, reports, cancellation, Leader instructions, blocking messages
and controller or coordination notices retain native delivery.

Notifications use a compact text header followed by a readable body:

```text
<kind> <message> <sender> [<task>.<revision>] [<status>] [re=<message>] [!=blocks]
<body or summary>
```

Kinds are `n` note, `q` question, `a` answer, `r` report and `c` cancellation.
The recipient and run are implicit. Structured reports inline their status,
summary, unresolved issues and an artifact reference; artifact evidence and
checks remain available through `read_message`. Revision previews may use twelve
characters alongside a resolvable `h` alias. Large bodies end with an explicit
`read_message` reference. A preview is not complete evidence.

`read_message` takes `message_id` and returns the original immutable message with
compact identity fields and its complete body. Only its sender or recipient in
the assigned run can fetch it through a session credential. Native transport
acceptance records receipt; an explicit `acknowledge` is optional. Neither receipt
nor reading completes work. Receivers must not repeat effects for an already
handled message or send acknowledgement-only replies.
