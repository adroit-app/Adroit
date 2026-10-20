# Working areas

Working areas identify physical directories independently of run and session
ownership. Several runs can intentionally share an area. Registering the same
ordinary directory associates its existing identity with each run; registration
does not grant a write lease or serialize their work.

Task preparation uses a native jj workspace when the source belongs to jj,
including a colocated repository. Otherwise a Git source uses a Git worktree.
Without either VCS, preparation uses the selected ordinary directory and its
existing inputs. It does not initialize a repository or copy the input tree.

Native areas record their complete base revision and task description before a
Worker starts. The default jj base snapshots the source's current working
changes; an explicit revision selects that recorded revision. Git's default
base is its recorded HEAD commit. Reviewing uncommitted Git inputs requires
explicitly reusing their existing area rather than assuming a new worktree
contains them.

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
