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

Still unverified: Adroit-owned TUI, working and approval/question dialogs,
resume, model changes in simultaneous sessions, and runtime enforcement of the
Leader's subagent restriction. The installed version differs from the 0.163.0
version recorded during planning.

The exact release source was inspected locally at tag `rust-v0.162.1`.
`session_queue_commands.rs` routes queue traffic through the shared app server
and refuses `--no-daemon`; a separate embedded app server is not an equivalent
transport. The CLI generates its own client message ID. A failed or interrupted
queue command must therefore be treated as uncertain, without automatic retry.

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
expired`. Transport receipt and wake-up were observed; model acknowledgement,
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
