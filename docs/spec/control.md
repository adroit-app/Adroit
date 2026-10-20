# Control requests

The terminal daemon also owns orchestration state. Closing the window does not
end coordination. If the controller cannot open its state, ordinary terminal
sessions remain available and orchestration reports its failure.

Local integrations submit a JSON command on standard input to `adroit control`.
Output is JSON containing `request_id` and a `result`, represented as either
`{"Ok": value}` or `{"Err": {"code": code, "message": message}}`. A failed request
returns a nonzero exit status. `--request-id UUID` preserves an operation's ID
across reconnections; retry state changes with the same ID and content.

Commands use an `op` field. Read commands are `snapshot` and `events` (with
`after` and `limit`). Event pages contain `events` and `next_cursor`; resume
from that cursor even when a page contains no visible events. A snapshot carries
its event cursor so subsequent changes can be replayed.

Session credentials supplied in `ADROIT_CONTROL_TOKEN` determine the caller's
identity and generation. They cannot select another actor, register Leaders, or
issue credentials. Session reads expose their assigned run and their own
messages. The local human client can select a session using
`--actor UUID GENERATION`; without a session credential it uses the instance's
private human credential. These credentials prevent accidental misrouting;
they do not isolate hostile programs running as the same operating-system user.

The underlying socket protocol is versioned JSON lines. Requests contain
`version`, `id`, `credential`, nullable `actor`, and `command`. Responses contain
`version`, nullable `id`, and `result`. Version 1 supports the command forms
below; IDs are UUID strings and session references contain `id` and `generation`.

- `register_leader`: `id`, `agent`, `cwd`, optional `permissions` (human client).
- `grant`: `identity` (human client; returns the session credential).
- `remember_launch`: `identity`, `runtime` containing a `settings` snapshot (human client; recorded before startup and immutable for that generation).
- `attach_session`: `identity`, `cwd`, `terminal_id`, `conversation_id` (human
  client; records a completed startup).
- `fail_start`: `identity`, `reason` (human client; records a failed startup).
- `start_uncertain`: `identity`, `reason` (human client; retains uncertain startup for reconciliation).
- `snapshot`: no additional fields.
- `mailbox`: `after`, `limit`; returns a recipient-only page with `messages` and
  `next_cursor`. Its sequence cursor is independent of the event cursor.
- `claim_delivery`: `message_id`; claims one stored message for transport.
- `accept_delivery`: `message_id`; records transport acceptance for the caller.
- `retry_delivery`: `message_id`; the run's Leader explicitly retries an accepted
  or uncertain delivery using the same immutable message ID.
- `events`: `after`, `limit`.
- `open_run`: `id`, `brief`.
- `put_task`: `plan`.
- `queue_task`: `version` containing `task_id` and `revision`.
- `send_message`: `message`.
- `acknowledge`: `message_id`.

State-changing commands validate the caller's run ownership and task revision.
A transport timeout does not prove an operation failed. Message acceptance and
explicit recipient acknowledgement are separate facts.

Frames are limited to 1 MiB including the terminating newline. Oversized
responses return an error, rather than an incomplete snapshot. Attachments and
large artifacts should be referenced from their working directories.

## Native agent bridge

`adroit control mcp` serves MCP tools over standard input and output using the
session credential. It never falls back to a human credential. The `control`
tool takes `request_id` and `command` using the command forms above.

`adroit control mcp --channel` also exposes Claude channel notifications. The
bridge reads the recipient's durable mailbox in the background; the Leader
never needs to hold a model turn open waiting for messages. Writing a notification
records transport acceptance. Only an explicit `acknowledge` command records
receipt. Unacknowledged messages remain in the mailbox with their original IDs. After a
daemon restart, unfinished dispatches become uncertain rather than being
silently sent again. The Leader can inspect the state and request a retry.
Receivers must avoid repeating effects for an ID they have already handled.

Message bodies are limited to 64 KiB. Mailbox pages are bounded by encoded size
as well as entry count, so one page cannot silently skip large messages. Use
file references for larger artifacts.

## Worker outcomes

A Worker reports a finished attempt explicitly with `report_task`, whose `report`
contains `message_id`, `task` (`task_id` and `revision`), `attempt_id`, and
`result`. Results and their Leader notifications are durable together. A model
turn ending does not finish the task, and a successful Worker does not complete
the run. Late results remain inspectable without completing revised work.

`result` contains:

- `status`: `succeeded`, `failed`, `blocked`, or `cancelled`.
- `summary`: a nonempty explanation.
- `artifact`: nullable; required for success. A report artifact has `kind` equal
  to `report`, arbitrary JSON `data`, a `files` array, and nullable
  `reviewed_revision`. A change artifact has `kind` equal to `change`, `backend`
  (`jj`, `git`, or `directory`), `work_area_id`, nullable `revision`, a `manifest`
  array of file references, and nullable `candidate_fingerprint`. VCS artifacts
  require a full immutable commit ID; directory artifacts require a nonempty
  manifest and fingerprint.
- `checks`: an array of objects containing a `command` argument array,
  `exit_code`, and the `candidate` identity checked.
- `unresolved`: an array of remaining issues; empty for success.

`cancel_task` takes a task `version` and requests cooperative cancellation.
The attempt remains active until stopping is confirmed. Cancellation does not
close the Agent terminal or its associated Shell and Neovim terminals. Updating
instructions invalidates the previous assignment and requests that it stop;
its result cannot satisfy the new revision. A finished task revised for new
work starts from the planned state.

Managed provider sessions receive an MCP server named `adroit_control` in their
launch configuration. It uses the assigned session credential and endpoint;
provider configuration files are not rewritten. Reserved `ADROIT_` identity
and control variables come from the assignment, taking precedence over inherited
or configured values. Credentials are passed through the process environment,
not through command arguments or terminal tags.

The Claude channel transport uses Claude Code's development-channel opt-in and
retains its native warning and organization policy checks. It is not an approved
channel plugin, and unsupported or denied channel activation does not count as
successful message delivery. Existing MCP servers remain configured alongside
Adroit's bridge. Codex receives a session-local MCP configuration and uses its
native queue for inbound notifications.

`session start` creates human-owned sessions. A managed Agent delegates with
`put_task` and `queue_task`; it cannot use `session start` to create a peer
Leader without a task assignment.

The daemon supervises queued tasks and native notifications without a window.
It keeps a private snapshot of each session's launch settings, so editing the
current provider settings does not change the route of an existing session.
Routing snapshots are omitted from public snapshots and event payloads.
Notifications to one native recipient are serialized, and an unobservable
transport result remains uncertain until explicitly retried.

A native conversation discovered after startup is associated with its existing
logical session. A later conversation change requires reconciliation rather
than transferring the previous task. This tracking does not require a window.
