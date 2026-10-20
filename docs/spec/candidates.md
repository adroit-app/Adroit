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
