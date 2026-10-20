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
