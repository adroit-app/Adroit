# Native session lifecycle

A confirmed native terminal exit is a session exit, never successful task work.
An unfinished Worker attempt becomes failed or cancelled and releases its slot.
An already recorded outcome remains available, and a newer task revision is not
overwritten. A Leader's exit leaves its run and other Workers available.

A missing terminal alone does not prove that a program exited. Its assignment
needs reconciliation, and an uncertain active attempt retains its slot. Adroit
notifies the Leader without automatically starting a replacement. A later
confirmed exit can release that slot. Repeated observations do not repeat the
same state transition or question.

Changing the native conversation also requires reconciliation. An affected Agent
may read its context, snapshots, mailbox, candidate, and events, but cannot change
orchestration state or claim deliveries through its session credential. Human
control remains available. Already finished work is preserved; losing its
terminal does not by itself invalidate its evidence or block run completion.

After an explicit Stop Session succeeds, the window records the confirmed exit
with the daemon. The human-only `confirm_terminal_exit` command takes `identity`
and `terminal_id`; both must identify the current assigned terminal. It records
a confirmed fact and does not stop a program itself. Closing tabs or the window
does not confirm an exit or stop Agent and Terminal sessions.

## Interrupted preparation

Controller startup records which sessions were still preparing when its endpoint
was bound. Recovery does not include sessions created after that boundary, and
an already attached session is not failed using an earlier preparation record.

Preparation interrupted before startup intent is recorded becomes failed or
cancelled without starting another attempt. A recorded startup intent remains
uncertain and retains its slot, including when cancellation was already
requested. A partially prepared working area is kept for native reconciliation;
a late preparation result cannot silently make it ready again. Late startup
uncertainty cannot revive a confirmed exited or failed session.

## Refused startup gates

If ownership preparation is refused and Adroit confirms stopping the gated
terminal, the native provider was never released. That startup is failed or
cancelled and its attempt slot is released, including when attachment was saved
but its reply was lost. If stopping the gate cannot be confirmed, startup remains
uncertain. A failure after release is not treated as a confirmed gate abort.

The human-only `abort_prepared_start` command records this confirmed fact using
`identity`, `terminal_id`, and `reason`. A different assigned terminal is refused.
It does not turn a failed launch into successful work or overwrite a later task
revision.

## Launch snapshots

Before startup, native routing settings and the original target selection are
saved for that session generation. Requested model, selected model, selection
source, agent, and permissions remain distinct and do not follow later changes
to global settings. A snapshot cannot substitute another agent, permissions, or
assigned attempt's target, and is immutable within its generation.

These values describe launch intent. They do not establish the actual model
used by the provider; an unreported actual model remains unknown.

Codex role instructions are appended to the effective developer instructions
read from its native configuration for the working area. The original base is
kept in the private launch snapshot. Managed startup refuses an unavailable or
unsupported configuration response rather than replacing an unknown user policy.
An oversized composed instruction is refused without truncating that policy.
The configuration read does not create a model thread or turn, and does not
change user or project configuration files.
