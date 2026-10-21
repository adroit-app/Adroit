# Working areas

Working areas identify physical directories independently of run and session
ownership. Several runs can intentionally share an area. Registering the same
ordinary directory associates its existing identity with each run; registration
does not grant a write lease or serialize their work.

Automatic preparation for a read-only Worker reuses the selected live source
area, including its uncommitted and untracked inputs, unless the task pins a
base revision or checks a candidate. An explicit existing area remains the
selected area. Sharing a source does not change permissions or ownership.

Other automatic task preparation uses a native jj workspace when the source
belongs to jj, including a colocated repository. Otherwise a Git source uses a
Git worktree. Without either VCS, preparation uses the selected ordinary
directory and its existing inputs. It does not initialize a repository or copy
the input tree.

New native areas record their complete base revision and task description
before a Worker starts. The default jj base snapshots the source's current
working changes; an explicit revision selects that recorded revision. Git's
default base is its recorded HEAD commit; a new worktree does not include
uncommitted source inputs.

An explicit destination wins over placement defaults. Colocated and Git sources
respect `wt.basedir`; otherwise new areas live under a dedicated sibling
container. Names identify the new area independently of session labels.

A failed VCS operation never redirects a Worker into the source checkout. A
partly created area remains recorded for inspection. Existing destinations are
not overwritten, and areas inside VCS metadata are refused. Preparation and
startup state survive closing the window and reopening the controller.

A Worker starts only inside a ready area associated with its run. Shell and
Neovim resources can use that same directory without creating another native
working area.

Queued work is prepared and started in the background. Execution slots belong to
attempts across runs; sharing a directory does not create another slot limit.
Working-area preparation failures are reported to the Leader without launching
a Worker in the source as a fallback. When startup may already have happened,
Adroit retains the attempt for reconciliation instead of treating a missing
acknowledgement as proof that no process exists.
