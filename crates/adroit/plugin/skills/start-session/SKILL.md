---
name: start-session
description: Delegate work to an ordinary Adroit Worker session with its own agent and model, visible and interruptible by the user. Leaders use typed control tasks; Workers report to their assigned Leader.
---

# Delegate work in Adroit

First allocate request IDs with `new_ids`, then use Adroit's native `control`
tool to read `context` using one of those IDs before planning. It identifies
your role, run, task, attempt, and working area. Use the tool's complete input
schema rather than guessing fields. A Leader normally already has a run: update
its brief with `update_run` rather than opening another run.

A Leader delegates investigation, implementation, review, classification,
integration, and verification to external Workers. Do not use internal subagents
in a Leader. A Worker may use native internal subagents within its assignment;
ask its Leader with `send_message` when another external task is needed.

## Dispatch a task

Use `new_ids` for fresh request and task IDs. `put_task` takes `plan` containing
`id`, `run_id`, `expected_revision`, `brief`, and `dependencies`. New tasks have a
null expected revision. The brief carries instructions, scope, inputs, purpose,
working-area policy, and target preference. `target.agent` and `target.model`
select that Worker independently of the Leader. Omitted preferences use that
provider's configured defaults, without changing another session.

Use `queue_task` with the returned task ID and revision. The daemon reserves an
attempt, prepares a jj workspace or Git worktree when available, and opens the
ordinary native Worker session. A plain directory is also supported. Existing
working areas must be selected explicitly when a fresh native area is unsuitable.

Request IDs identify immutable commands. Keep the same request ID when retrying
a command whose response was lost; use a fresh one for another command or changed
content. Task, attempt, run, and message IDs identify ownership. Session names,
working directories, and conversation ancestry do not.

## Receive results without waiting

Finish the Leader turn after dispatch. Native notifications wake it when results,
questions, or problems arrive. Acknowledge each incoming immutable message ID
before acting, and avoid repeating effects already handled. Native transport
acceptance is distinct from acknowledgment and task completion.

Workers use `send_message` for progress and questions, including their task,
attempt, and revision. A blocking question sets `blocks_work`; its answer uses
`reply_to`. Final outcomes use `report_task` with a structured result, actual
checks, and unresolved issues. A plain report message does not finish a task.

For changes, use the selected candidate and candidate-bound review and
verification tasks. A review's explicit decision distinguishes completing the
review from approving the change. Final completion is confirmed by the daemon;
never declare success solely because a Worker said it finished.

Humans can interrupt any Worker in its native console. Preserve their changes and
report material scope changes to the Leader. Coordinate intended default-area
changes and outcomes with related Leaders; do not require locks or all-Leader
acknowledgments for independent work.

## CLI fallback

If native control tools are unavailable, the bounded control command uses this
session's scoped credential:

```sh
printf '%s\n' '{"op":"context"}' | @ADROIT@ control
```

For mutations, pass a stable `--request-id` and the same command JSON when
retrying. Do not bypass the control task API to create peer Leaders. A missing
managed context requires user reconciliation rather than inferred ownership.
