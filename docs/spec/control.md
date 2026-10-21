# Control requests

The terminal daemon also owns orchestration state. Closing the window does not
end coordination. If the controller cannot open its state, ordinary terminal
sessions remain available and orchestration reports its failure.

Local integrations submit a JSON command on standard input to `adroit control`.
Output is JSON containing `request_id` and a `result`, represented as either
`{"Ok": value}` or `{"Err": {"code": code, "message": message}}`. A failed request
returns a nonzero exit status. `--request-id UUID` preserves an operation's ID
across reconnections; retry state changes with the same ID and content.

Commands use an `op` field. Read commands are `context`, `snapshot`, and `events` (with
`after` and `limit`). Event pages contain `events` and `next_cursor`; resume
from that cursor even when a page contains no visible events. A snapshot carries
its event cursor so subsequent changes can be replayed.

Session credentials supplied in `ADROIT_CONTROL_TOKEN` determine the caller's
identity and generation. They cannot select another actor, register Leaders, or
issue credentials. Session reads expose their assigned run and their own
messages. A human-token socket client has read-only access and may select a
session for observation. The CLI requires a managed session credential;
window and internal startup actions use an in-process store path for lifecycle
changes. These credentials prevent accidental misrouting;
they do not isolate hostile programs running as the same operating-system user.

The underlying socket protocol is versioned JSON lines. Requests contain
`version`, `id`, `credential`, nullable `actor`, and `command`. Responses contain
`version`, nullable `id`, and `result`. Version 1 supports the command forms
below; IDs are UUID strings and session references contain `id` and `generation`.

- `register_leader`: `id`, `agent`, `cwd`, optional `permissions` (app only).
- `grant`: `identity` (app only; returns the session credential).
- `register_default_area`: `identity`, `run_id`, ready `area` (app only; associates the existing default before startup).
- `remember_launch`: `identity`, `runtime` containing a `settings` snapshot (app only; recorded before startup and immutable for that generation).
- `attach_session`: `identity`, `cwd`, `terminal_id`, `conversation_id` (app
  only; records a completed startup).
- `fail_start`: `identity`, `reason` (app only; records a failed startup).
- `start_uncertain`: `identity`, `reason` (app only; retains uncertain startup for reconciliation).
- `context`: no additional fields; returns the credential's own session, current run and task, assigned attempt, and working area without message history.
- `snapshot`: no additional fields.
- `mailbox`: `after`, `limit`; returns a recipient-only page with `messages` and
  `next_cursor`. Its sequence cursor is independent of the event cursor.
- `claim_delivery`: `message_id`; claims one stored message for transport.
- `accept_delivery`: `message_id`; records transport acceptance for the caller.
- `retry_delivery`: `message_id`; the run's Leader explicitly retries an accepted
  or uncertain delivery using the same immutable message ID.
- `events`: `after`, `limit`.
- `open_run`: `id`, `brief` (app only; Agents cannot open an unassigned Leader run).
- `put_task`: `plan`.
- `queue_task`: `version` containing `task_id` and `revision`.
- `send_message`: `message`.
- `acknowledge`: `message_id`.

State-changing commands validate the caller's run ownership and task revision.
The socket refuses all state changes under the readable human credential,
including native command wrappers and combinations of lifecycle commands.
A transport timeout does not prove an operation failed. Native transport acceptance records receipt; explicit acknowledgement is
optional. Receipt does not establish processing or task completion.

Frames are limited to 1 MiB including the terminating newline. Oversized
responses return an error, rather than an incomplete snapshot. Attachments and
large artifacts should be referenced from their working directories.

## Native agent bridge

`adroit control mcp` serves MCP tools over standard input and output using the
session credential. It never falls back to a human credential. Each operation is a named tool described in [control-tools.md](control-tools.md).

`adroit control mcp --channel` also exposes Claude channel notifications. The
bridge reads the recipient's durable mailbox in the background; the Leader
never needs to hold a model turn open waiting for messages. Writing a notification
records transport acceptance and receipt. Messages remain in the mailbox with
their original IDs regardless of receipt. After a
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
The attempt remains active until stopping is confirmed. A final report closes
the Worker automatically; associated Shell and Neovim terminals remain available. Updating
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

`session start` can create unassigned sessions directly. A CLI request that
would automatically create a managed Leader requires an explicit window action;
the command refuses it and directs the user to the window. A managed Agent
delegates with `put_task` and `queue_task`.

The daemon supervises queued tasks and native notifications without a window.
It keeps a private snapshot of each session's launch settings, so editing the
current provider settings does not change the route of an existing session.
Routing snapshots are omitted from public snapshots and event payloads.
Notifications to one native recipient are serialized, and an unobservable
transport result remains uncertain until explicitly retried.

A native conversation discovered after startup is associated with its existing
logical session. A later conversation change requires reconciliation rather
than transferring the previous task. This tracking does not require a window.

Task attempts record the revisions of their dependency inputs. Revising an input
invalidates dependent work, including completed results, while independent work
remains scheduled. Running dependents receive a stop request and keep their
execution slots until stopping is confirmed. Old results remain in history;
they cannot restore the new plan to success or invalidate it a second time.

## Updating and cancelling a request

`update_run` takes `run_id`, `expected_revision` (the current plan revision),
`brief`, and an optional `affected` array of task versions. The caller declares
which tasks are affected by the new direction. Those tasks and their dependents
are invalidated together; independent tasks retain their progress. A stale
revision or a foreign task refuses the whole update.

`cancel_run` takes `run_id`. It cancels pending work and requests that active
Workers stop. The run stays `cancelling` while any attempt still holds an
execution slot, and becomes `cancelled` after stopping is confirmed. A late
success does not override cancellation. A different run in the same directory
keeps running. Workers close automatically after their final reports. Associated Shell and
Neovim resources remain available; explicit terminal stop remains separate.

A Leader cannot open another run while its current run is still cancelling.
Independent requests can start in separate Leaders without waiting for it.

## Completing a report run

`complete_run` takes `run_id`, `expected_revision`, and a nonempty `summary`.
It records the Leader's final summary against the current plan and current
required results. Required tasks default to true; supplementary tasks may set
`brief.required` to false. Unstarted supplementary work is skipped on completion.

Completion requires no active attempts or unanswered current blocking questions.
An answer identifies its question with `reply_to` and addresses the original
sender. Obsolete assignments cannot hold a revised request open. Finishing a
run leaves its interactive Agent and tool terminals available.

Report completion requires no VCS commit. Change runs and change artifacts
require a separate verified candidate and cannot use report-only completion.

## Shared default changes

Before editing, applying, or merging into its default working area, a Leader
uses `announce_change` with `intent`. The intent contains `id`, `run_id`,
`work_area_id`, `operation` (`edit`, `apply`, or `merge`), `purpose`, `scope`, and
nullable `base` and `candidate`. Its default area must already be ready.

Related Leaders receive the saved intent through their ordinary message
transport. Leaders sharing the physical location or repository are related;
independent locations do not receive each other's coordination. The caller
gets an immediate response after persistence. No reply grants permission or
holds a directory lock, and unanswered notices do not block other runs.

`discuss_change` takes a `reply` containing `id`, `notice_id`, `from` (a session
reference), `run_id`, nullable `task_id`, and `body`. Related Leaders may discuss
the concrete scope. Only the announcing Leader can use `finish_change` with
`notice_id` and `outcome`. The outcome contains `summary`, actual `scope`, nullable
`revision` and `fingerprint`, and `unresolved`. It records failures and conflicts
as well as successful changes, and notifies the related Leaders.

Snapshots retain these notices and discussions for the participating runs.
Coordination does not reassign tasks or merge the participating requests.

New managed Leaders register their existing default working area before their
native program starts. Registration identifies the current native jj workspace,
Git worktree, or selected ordinary directory; it does not create another working
area or record user edits in history. Independent Leaders at the same physical
default share its working-area identity. A newly registered related Leader
receives the saved coordination context without delaying existing work.

## Worker file evidence

Workers can call the MCP tool `fingerprint_files` with a `paths` array to obtain
the canonical directory output fingerprint. The CLI equivalent is
`adroit control fingerprint`, taking the same manifest array on standard input
and returning a JSON result string. It requires a session credential and never
falls back to the human client.

The helper resolves the current assignment from that credential. It accepts no
working-directory or session-ID override. Leaders, stopped attempts, and revised
assignments are refused. File hashing runs outside the durable state owner;
other sessions can continue issuing control requests. The helper returns the
fingerprint rather than file contents.

The human UI reads compact pages of controller state and loads full evidence on
request. Saved history remains intact. Inconsistent pages are retried together;
the UI keeps its last complete view when a refresh fails.

A running daemon without UI paging support can keep serving live terminals while
the human UI reads a compatible database read-only. Mutation requests continue to
use the authenticated daemon. Unsupported schemas and read failures remain
visible as controller errors.
