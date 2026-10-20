# Candidate file evidence

Candidate file evidence identifies declared outputs and verification inputs.
It includes file content, executable status, symbolic link targets, and missing
files. Changes outside the declared paths do not alter this evidence. Declared
inputs must include everything relevant to the requested verification.

A manifest is an array of unique relative file paths using `/` separators.
Empty components, `.` and `..`, absolute paths, VCS metadata, and directories
are refused. Parent directories cannot be symbolic links. A symbolic link
listed as an entry is recorded as a link; its target is never opened.

Version 1 evidence contains `version`, an opaque `fingerprint`, and `files`.
Files are ordered by path. Each entry contains `path` and `state`. The state has
`kind` equal to `file`, `symlink`, or `missing`. A file also carries its content
`sha256` and boolean `executable`; a symlink carries its `target`. The aggregate
fingerprint identifies both the declared paths and their recorded states.

Capture refuses unreadable files, unsupported file types, detected concurrent
file changes, and manifests exceeding its bounds. It does not lock directories,
provide rollback, or guarantee an atomic filesystem snapshot.

## Native and directory candidates

A native candidate names a full immutable commit ID and its assigned working
area. Its current tree must match that commit, including the absence of
unrecorded Git files and unresolved jj conflicts. An empty jj successor with
the same tree is accepted. Checks leave the active history and working files
unchanged; a missing native capability or a failed check refuses the candidate.

Directory candidates use the reported output manifest and its canonical file
fingerprint. A stale or unsupported reported fingerprint is refused. Additional
verification inputs are captured with the outputs, so later verification can
identify changes to those inputs independently of the output report.

## Selecting a completion candidate

A run's `brief.quality_gate` contains `commands` (argument arrays) and
`review_required` (a boolean). Code-change completion needs the declared quality
conditions; reports do not require code-change checks. A candidate requires at
least one command or a required review.

`propose_candidate` takes `expected_revision` (the run's current plan revision)
and `plan`. The plan contains `id`, `run_id`, `source` (a task version),
`attempt_id`, and optional additional file `inputs`. Its source must be the
current applied successful change result. A stale plan, foreign source, or
missing quality condition refuses the proposal.

Selecting a candidate advances the plan revision and supersedes its previous
selection. Source results remain historical evidence; selection does not
reassign them. A proposal is durable before capture starts, and proposing it
does not complete the run. Revising or cancelling its source invalidates the
candidate. A late capture cannot restore it to the current plan.

Capture records a prepared proof or a failure and notifies the Leader. Prepared
file evidence is immutable. Verification and final completion must use the
current selected candidate and current source revisions.

## Background capture and public summaries

The daemon executes pending capture outside the state owner, independently of
the window and the Leader's model turn. These reads may be repeated after a
controller restart. A failed candidate does not hold up another run's capture.
Cancelling the run invalidates its pending candidate without closing its Agent
or Terminal sessions.

`candidate` takes `id` and returns a candidate summary within the caller's run.
Snapshots, candidate responses, and candidate events expose the source artifact,
state, error, and proof identity with its file count. They omit the full list of
captured file hashes; detailed proof remains in durable state. The Leader is
notified when capture finishes, and preparation alone is not run completion.
