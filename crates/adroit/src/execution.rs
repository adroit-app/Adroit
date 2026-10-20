//! Launch preparation shared by the window, commands and control service.

use std::path::{Path, PathBuf};

use adroit_model::{
    Agent, AgentPermissions, CustomPermissions, TerminalBinding, TerminalProgram,
    TerminalResourceId, TerminalSession,
};
use adroit_term::{Daemon, Palette, Pane, Size, Spawn};

use crate::{agents, sessions, settings::Settings};

#[derive(Debug, Clone, Default)]
pub struct StartRequest {
    pub agent: Agent,
    pub model: Option<String>,
    pub name: String,
    pub cwd: PathBuf,
    pub prompt: String,
    pub images: Vec<PathBuf>,
    pub permissions: AgentPermissions,
    pub binding: Option<adroit_model::SessionBinding>,
    pub env: Vec<(String, String)>,
    pub started_ms: u64,
    pub started_from: Option<String>,
}

pub use adroit_model::ModelSource;

#[derive(Debug)]
pub struct ResolvedLaunch {
    pub agent: Agent,
    pub requested_model: Option<String>,
    /// The launch selection, not an observation of the provider's actual model.
    pub model: Option<String>,
    pub model_source: ModelSource,
    pub conversation: String,
    /// Policy requested at launch; this is not proof of runtime enforcement.
    pub role_version: Option<u32>,
    pub spawn: Spawn,
}

#[derive(Debug, Clone, Default)]
pub struct TerminalStartRequest {
    pub name: String,
    pub cwd: PathBuf,
    pub program: TerminalProgram,
    pub binding: Option<TerminalBinding>,
    pub env: Vec<(String, String)>,
}

pub struct ResolvedTerminalLaunch {
    pub terminal: TerminalSession,
    pub spawn: Spawn,
}

pub fn resolve_terminal(
    settings: &Settings,
    request: TerminalStartRequest,
    login_shell: &str,
) -> ResolvedTerminalLaunch {
    let shell = settings.shell.program(login_shell);
    let (program, args, env) = match request.program {
        TerminalProgram::Shell => {
            let (program, args) = settings.shell.command_line(login_shell);
            let env = [("SHELL".into(), program.clone())]
                .into_iter()
                .chain(settings.shell.env.clone())
                .collect::<Vec<_>>();
            (program, args, env)
        }
        TerminalProgram::Neovim => {
            let (program, args) = settings.neovim.command_line("nvim", &[], &shell);
            (
                program,
                args,
                settings.neovim.env.clone().into_iter().collect(),
            )
        }
    };
    let resource_id = TerminalResourceId::new();
    let tags = sessions::terminal_tags(
        &request.name,
        &request.cwd,
        request.program,
        resource_id,
        request.binding.as_ref(),
    );
    ResolvedTerminalLaunch {
        terminal: TerminalSession {
            resource_id,
            id: 0,
            name: request.name,
            cwd: request.cwd.clone(),
            program: request.program,
            binding: request.binding,
        },
        spawn: Spawn {
            program,
            args,
            cwd: request.cwd,
            env: request.env.into_iter().chain(env).collect(),
            tags,
            ..Spawn::default()
        },
    }
}

pub async fn spawn_headless(
    daemon: &Daemon,
    spawn: Spawn,
    palette: &Palette,
) -> Result<u128, String> {
    let size = Size::new(spawn.cols, spawn.rows);
    let pane = Pane::start(daemon, spawn, size, palette, |_| || {})
        .await
        .map_err(|e| e.to_string())?;
    Ok(pane.id())
}

pub fn resolve(
    settings: &Settings,
    request: StartRequest,
    adroit_program: &Path,
) -> Result<ResolvedLaunch, String> {
    let mut settings = settings.clone();
    match (&request.permissions, request.agent) {
        (AgentPermissions::Default, _) => {}
        (AgentPermissions::ReadOnly, Agent::Codex) => {
            settings.codex_sessions.sandbox_mode = "read-only".into();
        }
        (AgentPermissions::ReadOnly, Agent::Claude) => {
            return Err("read-only enforcement is not supported for Claude Code; plan mode alone does not enforce it".into());
        }
        (AgentPermissions::Custom(profile), agent) => match (profile.as_ref(), agent) {
            (
                CustomPermissions::Claude {
                    permission_mode,
                    disallowed_tools,
                },
                Agent::Claude,
            ) => {
                settings.sessions.permission_mode = permission_mode.clone();
                settings.sessions.disallowed_tools = disallowed_tools.clone();
            }
            (
                CustomPermissions::Codex {
                    approval_policy,
                    sandbox_mode,
                },
                Agent::Codex,
            ) => {
                settings.codex_sessions.approval_policy = approval_policy.clone();
                settings.codex_sessions.sandbox_mode = sandbox_mode.clone();
            }
            _ => return Err("custom permissions belong to another agent".into()),
        },
    }
    let model = match request.agent {
        Agent::Claude => &mut settings.sessions.model,
        Agent::Codex => &mut settings.codex_sessions.model,
    };
    let source = match &request.model {
        Some(given) => {
            if given.trim().is_empty() {
                return Err("the model is empty".into());
            }
            *model = given.trim().to_string();
            ModelSource::Explicit
        }
        None if model.trim().is_empty() => ModelSource::AgentDefault,
        None => ModelSource::Settings,
    };
    let model = (!model.trim().is_empty()).then(|| model.trim().to_string());
    let adapter = agents::adapter(request.agent);
    let conversation = adapter.conversation();
    let mut args = adapter.integration_args(adroit_program);
    let role_version = request.binding.as_ref().map(|binding| {
        args.extend(adapter.role_args(
            &mut settings,
            binding.role(),
            &crate::roles::instructions(binding),
        ));
        crate::roles::VERSION
    });
    args.extend(adapter.image_args(&request.images));
    args.extend(adapter.start_args(
        &settings,
        &request.name,
        &conversation,
        &adapter.image_prompt(&request.prompt, &request.images),
    ));
    let (program, args, env) =
        agents::launch_with_settings(&settings, request.agent, args, request.env);
    let mut tags = sessions::agent_tags(
        request.agent,
        &request.name,
        &request.cwd,
        &conversation,
        &request.permissions,
        request.started_ms,
        request.started_from.as_deref(),
    );
    if let Some(binding) = &request.binding {
        tags.extend(adroit_term::tags::tagged(
            "adroit.binding",
            &serde_json::to_string(binding).expect("serializable assignment"),
        ));
        tags.extend(adroit_term::tags::tagged(
            "adroit.binding-conversation",
            &conversation,
        ));
    }
    Ok(ResolvedLaunch {
        agent: request.agent,
        requested_model: request.model,
        model,
        model_source: source,
        conversation,
        role_version,
        spawn: Spawn {
            program,
            args,
            env,
            cwd: request.cwd,
            tags,
            ..Spawn::default()
        },
    })
}

pub use crate::agents::{DeliveryFailure, DeliveryFuture};

/// Submit to a native queue without touching the TUI's input buffer.
pub fn deliver(
    agent: Agent,
    settings: Settings,
    cwd: PathBuf,
    conversation: String,
    envelope: String,
) -> DeliveryFuture {
    agents::adapter(agent).deliver(settings, cwd, conversation, envelope)
}
