# Native session lifecycle

Adroit watches every Leader and Worker it starts while its provider starts. Its
operational readiness is reported separately from terminal presence, and a
Worker retains its attempt slot meanwhile. If the provider is waiting for
trust, permission, or any other dialog, Adroit shows the required action as soon
as it sees it and durably tells the request's Leader. The user decides in the
native terminal; Adroit does not approve trust or permission on their behalf. A
startup that neither reaches readiness nor waits on a dialog is shown as
stalled. Once the session becomes ready, the blocker clears and the Leader is
notified. Repeated observations do not create duplicate notices.

A confirmed native terminal exit is a session exit, never successful task work.
An unfinished Worker attempt becomes failed or cancelled and releases its slot.
An already recorded outcome remains available, and a newer task revision is not
overwritten. An unexpected Leader exit leaves its run available for explicit
recovery. Stopped Leaders are not retained as entries in session navigation.

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

After an explicit Stop Session succeeds, the window records the closure with the
daemon. Closing a Leader cancels unfinished work and automatically stops its
Workers. A manually closed Leader cannot be restarted. Closing a Worker ends its
attempt without inventing success. Results and working files remain available.
Closing tabs or the window does not stop Agent and Terminal sessions.

The app-only `close_session` command records a deliberate closure after the
native terminal has stopped. `confirm_terminal_exit` records an exit without
implying a manual closure. Both take `identity` and `terminal_id`, which must
identify the current assigned terminal; neither stops a program itself.

After a final Worker report is saved, Adroit automatically closes that Worker's
native terminal and releases its resources. This applies to every final outcome,
including results still undergoing candidate verification. The report and Leader
notification remain durable, and candidate verification continues independently.

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

A background startup job that stops unexpectedly follows the same recovery
boundary. It does not silently retain an unfinished preparation or launch a
replacement when native startup remains uncertain.

## Refused startup gates

If ownership preparation is refused and Adroit confirms stopping the gated
terminal, the native provider was never released. That startup is failed or
cancelled and its attempt slot is released, including when attachment was saved
but its reply was lost. If stopping the gate cannot be confirmed, startup remains
uncertain. A failure after release is not treated as a confirmed gate abort.

The app-only `abort_prepared_start` command records this confirmed fact using
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

## Restarting a stopped Leader

A human may explicitly restart an exited or failed Leader using `restart_leader`
with its current `identity`. A missing or uncertain native terminal must first be
reconciled as stopped. Workers cannot use this operation or restart themselves.

Restart opens a fresh conversation with the saved request context, tasks, reports,
and mailbox. It preserves the Leader's logical identity, provider, permissions,
and saved launch settings while advancing its generation and invalidating old
credentials. It does not replay an uncertain delivery automatically, restart
Workers, stop Terminal resources, or reinterpret a failed restart as failure of
an otherwise active request. The new Leader reads the saved context before
continuing orchestration. The former native conversation is not resumed.

Recovery is an explicit control operation, not a persistent session-list entry.
Closing a window or tab does not queue a restart. Repeated requests with the same
request ID cannot start another generation.

## Native conversation discovery

An explicitly assigned terminal may display a shortened conversation identifier.
Adroit treats that fragment as a lookup hint and resolves it through the saved
provider route, accepting only one matching native UUID. An ambiguous or
incomplete response does not establish a recipient. Working directories and
session names alone cannot establish that association.

Discovery runs in bounded background jobs. A slow provider lookup does not hold
an Agent turn, block control requests, or prevent other native observations.
A result for an old generation, terminal, or changed title is not applied to its
replacement. Repeated matching titles reuse the confirmed conversation identity.
