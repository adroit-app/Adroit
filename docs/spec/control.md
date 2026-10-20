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

- `register_leader`: `id`, `agent`, `cwd` (human client).
- `grant`: `identity` (human client; returns the session credential).
- `snapshot`: no additional fields.
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
