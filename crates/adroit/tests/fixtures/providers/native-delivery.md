# Native delivery probes

Observed on 2026-10-21 in disposable directories. These observations do not
constitute acceptance of the complete orchestration workflow.

## Codex 0.162.1

The native interactive CLI started with an explicit `gpt-6-luna` model and a
read-only sandbox. Its initial reply was `READY-PROBE`. The composer then held
the unsent text `UNSENT-HUMAN-DRAFT-2874`.

A separate `codex queue --thread <conversation UUID> --message <probe>` process
returned success. The existing TUI started a turn without an Enter key, replied
`RECEIVED-PROBE-002`, and still displayed the unsent draft unchanged. The target
was the UUID obtained from this probe's own session metadata, never a name.
The probe session was then explicitly closed.

An Adroit-owned end-to-end probe subsequently started a Leader with
`gpt-6-luna` and an external Worker with `gpt-6.1-sol`. The synthetic request
classified the word blue. The Worker reported a JSON object
`{"label":"color"}`; Adroit delivered the report to the idle Leader, which
acknowledged it and completed the report run with `NATIVE-SMOKE-COMPLETE`.
There was no manual input after the initial request. Both ordinary native
TUIs remained alive. Requested models were visible in their respective
native footers; the database's actual-model field remained unknown.

In that same Adroit-owned Leader, a second synthetic notification produced
`DRAFT-PRESERVED` while the unsent composer text
`UNSENT-ADROIT-DRAFT-7419` remained unchanged. Its message was explicitly
acknowledged and the completed run remained succeeded. The isolated
sessions and daemon were then explicitly stopped.

After the recovery review fixes, the synthetic workflow was repeated with
Codex native-origin checks enabled. It again completed without human input.
The Leader initially supplied a nonexistent source directory; startup failed
before a native Worker was released. The failure notification woke the Leader,
which revised the task, dispatched the replacement and completed revision 2.
The failed attempt remained in history alongside the applied successful report.

Still unverified: native working and approval/question dialogs, resume,
Claude model execution, and runtime enforcement of the Leader's native
subagent restriction. These probes used Codex 0.162.1, rather than the
0.163.0 version recorded during planning.

The exact release source was inspected locally at tag `rust-v0.162.1`.
`session_queue_commands.rs` routes queue traffic through the shared app
server and refuses `--no-daemon`. CLI configuration overrides can select an
embedded server for the TUI; its queue service polls shared SQLite for
external messages and wakes loaded threads. The actual probe confirmed
cross-process delivery. The CLI generates its own client message ID. A
failed or interrupted queue command must therefore be treated as uncertain,
without automatic retry.

Disabling only `features.multi_agent` is insufficient as an enforcement claim:
model metadata can select V2. The release's configuration also has
`agents.enabled` and `features.multi_agent_v2`. Their combined effect still
requires a runtime probe before automatic Leader operation is enabled.

## Claude Code 2.1.304

A disposable native interactive CLI loaded a local MCP server with the
`claude/channel` experimental capability and the development channel opt-in.
The server completed initialization and tools discovery. A channel notification
sent while idle appeared in the existing TUI and started processing without
pasting into its input buffer or pressing Enter.

The model could not produce an acknowledgement: the CLI reported `Login
expired`. A later minimal `--print --no-session-persistence --model haiku --tools ""`
probe also reported an OAuth session refresh failure. This describes the
probe process, not the authentication state of the user's own terminal.
Transport receipt and wake-up were observed; model acknowledgement,
draft preservation, model isolation, role enforcement, and the Adroit-owned TUI
remain unverified. The probe session was explicitly closed.

Channels are a research preview. The preview flags are deliberately absent
from CLI help; approved plugins and organization policy govern production use.
A locally developed channel working under the development opt-in does not
prove that every installation can load it by default.

Primary references:

- [Claude channels](https://code.claude.com/docs/en/channels)
- [Claude channel protocol](https://code.claude.com/docs/en/channels-reference)
- [Claude cross-session messaging](https://code.claude.com/docs/en/cross-session-messaging)
- [Codex CLI reference](https://learn.chatgpt.com/docs/cli/reference)
- [Codex release source](https://github.com/openai/codex/tree/rust-v0.162.1)

## Session-local MCP launch configuration

A separate Codex 0.162.1 app-server, using a disposable `CODEX_HOME`, accepted
Adroit's `-c mcp_servers.adroit_control.*` overrides through `config/read`.
The observed server command, argument array and `required` value matched the
launch configuration. No thread or model turn was started by this probe.

The compiled `adroit control mcp` process is also exercised over real stdio
against an isolated control service. It reaches the credential's run through
`ADROIT_CONTROL_ROOT` even when its XDG state directory points elsewhere. This
checks endpoint routing and tool requests; it does not establish that every
native provider successfully loads the bridge or responds to notifications.

## Vitro macOS acceptance

A dedicated disposable macOS 26.6.2 VM exercised the application with isolated
XDG directories and a deterministic interactive provider. Those GUI checks do
not call an external model or replace native provider acceptance above.

Observed behavior:

- Independent Leaders sharing a directory remain separate requests; each
  external Worker is shown under its assigned Leader. Different requested
  models are passed to separate provider processes.
- Workspaces and Leader / Workers expose the same console and draft. Hiding
  and reopening a session tab preserves its unsent prompt.
- Invalid configuration is visibly reported in both windows; changing a
  setting refuses to overwrite that file. Restoring a valid file recovers.
- Shell and ordinary Neovim run as Terminal resources and do not create
  Worker attempts. GUI-started Shell retained the selected Leader run;
  GUI-started Neovim retained the selected Worker binding. The console's
  embedded Neovim read mode remained available.
- Product-driven task startup prepared real jj workspaces and Git worktrees,
  preserving the selected base, repository association and Worker cwd.
- Explicitly restarting a stopped Leader retains its logical ID, run, task,
  existing Worker and attempt while advancing the Leader generation.
  Its unsent prompt was recovered after both a GUI restart and a separate
  CLI restart completed while the window was closed. Another request's
  composer remained empty after the first Leader stopped.

GUI testing uncovered ownership tags being removed during native conversation
discovery, draft transfer when hiding or stopping sessions, and zero start
timestamps on automatic launches. These received regression tests and
independent review. The final packaged executable is compared by SHA-256 with
the binary copied into the guest before accepting the rerun.

The final guest executable SHA-256 was
`0f50defdc9d865fbcaf63d0ae7da0a414e8b01c3ab9d55c8f224f937d55e65ca`.
The existing Worker attempt remained running through Leader generations 1, 2
and 3; starting its Shell and Neovim resources did not add attempts.
