//! Launch preparation shared by the window, commands and control service.

use std::path::{Path, PathBuf};

use adroit_model::profile::{ConnectionId, ProfileId};
use adroit_model::{
    Agent, AgentPermissions, CustomPermissions, TerminalProgram, TerminalResourceId,
    TerminalSession,
};
use adroit_term::{Daemon, Palette, Pane, Size, Spawn};

use adroit_agents as agents;

use crate::{delegation::launch::Delegation, sessions, settings::Settings};

#[derive(Debug, Clone, Default)]
pub struct StartRequest {
    pub agent: Agent,
    /// The profile it starts from, which has to start `agent`; `agent`'s
    /// default profile when none is named.
    pub profile: Option<ProfileId>,
    pub model: Option<String>,
    pub name: String,
    pub cwd: PathBuf,
    pub prompt: String,
    pub images: Vec<PathBuf>,
    pub permissions: AgentPermissions,
    /// How the session reaches other sessions; none for a launch Adroit
    /// keeps no record of.
    pub delegation: Option<Delegation>,
    /// Codex's own developer instructions, which Adroit's are added to.
    pub developer_instructions: Option<String>,
    pub env: Vec<(String, String)>,
    pub started_ms: u64,
    /// The conversation the session continues, by its id; a new one when
    /// none is given.
    pub resume: Option<String>,
    /// The connection that keeps the conversation it continues, which it
    /// has to run in.
    pub connection: Option<ConnectionId>,
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
    pub spawn: Spawn,
}

#[derive(Debug, Clone, Default)]
pub struct TerminalStartRequest {
    pub name: String,
    pub cwd: PathBuf,
    pub program: TerminalProgram,
    pub env: Vec<(String, String)>,
    /// The file to start Neovim on; a shell is started on none.
    pub file: Option<PathBuf>,
    /// The program and its arguments a command terminal runs.
    pub command: Vec<String>,
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
        TerminalProgram::Command => {
            // Through the shell, so the program finds what the user's startup
            // files set, as it would typed into a shell terminal.
            let launch = adroit_model::config::Launch {
                command: request.command.clone(),
                via_shell: true,
                env: settings.shell.env.clone(),
            };
            let (program, args) = launch.command_line("", &[], &shell);
            let env = [("SHELL".into(), shell.clone())]
                .into_iter()
                .chain(launch.env)
                .collect::<Vec<_>>();
            (program, args, env)
        }
        TerminalProgram::Neovim => {
            let file: Vec<String> = request
                .file
                .iter()
                .map(|f| f.to_string_lossy().into_owned())
                .collect();
            let (program, args) = settings.neovim.command_line("nvim", &file, &shell);
            (
                program,
                args,
                settings.neovim.env.clone().into_iter().collect(),
            )
        }
    };
    let file = request
        .file
        .filter(|_| request.program == TerminalProgram::Neovim);
    let command = if request.program == TerminalProgram::Command {
        request.command
    } else {
        Vec::new()
    };
    let terminal = TerminalSession {
        resource_id: TerminalResourceId::new(),
        id: 0,
        name: request.name,
        cwd: request.cwd.clone(),
        program: request.program,
        file,
        command,
    };
    let tags = sessions::terminal_tags(&terminal);
    ResolvedTerminalLaunch {
        terminal,
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
    spawn_headless_prepared(daemon, spawn, palette, |_| async { Ok(()) })
        .await
        .map_err(|error| error.to_string())
}

pub async fn spawn_headless_prepared<F>(
    daemon: &Daemon,
    spawn: Spawn,
    palette: &Palette,
    prepare: impl FnOnce(u128) -> F,
) -> adroit_term::Result<u128>
where
    F: std::future::Future<Output = adroit_term::Result<()>>,
{
    let size = Size::new(
        if spawn.cols == 0 { 120 } else { spawn.cols },
        if spawn.rows == 0 { 40 } else { spawn.rows },
    );
    let presence = Pane::start_unshown(daemon, spawn, size, palette, prepare).await?;
    let id = presence.id();
    crate::presences::keep(presence).await;
    Ok(id)
}

pub fn resolve(
    settings: &Settings,
    request: StartRequest,
    adroit_program: &Path,
) -> Result<ResolvedLaunch, String> {
    let (mut context, target) = configured(settings, &request)?;
    context.adroit = adroit_program.to_path_buf();
    if request.delegation.is_some()
        && let Some(options) = context.pi_mut()
        && options.tools.iter().any(|tool| !tool.trim().is_empty())
    {
        // Pi leaves an MCP tool its list does not name inactive, so a session
        // given its tools would lose those it reaches other sessions with.
        options.tools.push(crate::delegation::launch::TOOLS.into());
    }
    if let (Some(id), Some(kept)) = (&request.resume, &request.connection)
        && *kept != context.connection
    {
        // Another connection's program does not know the conversation, and
        // would run it under another login if it did.
        return Err(format!(
            "conversation {id} is kept in connection `{kept}`, which profile `{}` does not run in",
            context.profile
        ));
    }
    let adapter = agents::adapter(request.agent);
    let conversation = request
        .resume
        .clone()
        .unwrap_or_else(|| adapter.conversation());
    let mut args = adapter.integration_args(adroit_program, &crate::paths::instance_state());
    if let Some(delegation) = &request.delegation {
        args.extend(delegation.args(request.agent, adroit_program));
        let instructions = match (&request.developer_instructions, request.agent) {
            (Some(base), Agent::Codex) => {
                format!("{base}\n\n{}", delegation.instructions(adroit_program))
            }
            _ => delegation.instructions(adroit_program),
        };
        args.extend(adapter.instruction_args(&instructions));
    }
    match &request.resume {
        Some(id) => args.extend(
            adapter
                .resume_args(&context, id)
                .ok_or_else(|| format!("{} cannot resume a conversation", request.agent.label()))?,
        ),
        None => {
            args.extend(adapter.image_args(&request.images));
            args.extend(adapter.start_args(
                &context,
                &request.name,
                &conversation,
                &adapter.image_prompt(&request.prompt, &request.images),
            ));
        }
    }
    let (program, args, mut env) = agents::launch_in(&context, args, request.env);
    if let Some(delegation) = &request.delegation {
        env.retain(|(name, _)| !crate::env::reserved_delegation(name));
        env.extend(delegation.environment());
    }
    let started_ms = if request.started_ms == 0 {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_err(|error| format!("cannot determine the session start time: {error}"))?
            .as_millis() as u64
    } else {
        request.started_ms
    };
    let mut tags = sessions::agent_tags(
        request.agent,
        &request.name,
        &request.cwd,
        &conversation,
        &request.permissions,
        started_ms,
    );
    tags.extend(sessions::profile_tags(
        &context.profile,
        &context.connection,
    ));
    tags.extend(sessions::editor_tags(&env));
    if let Some(delegation) = &request.delegation {
        tags.extend(delegation.tags());
    }
    Ok(ResolvedLaunch {
        agent: request.agent,
        requested_model: request.model,
        model: target.model,
        model_source: target.model_source,
        conversation,
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

fn configured(
    settings: &Settings,
    request: &StartRequest,
) -> Result<(adroit_agents::AgentContext, adroit_model::TargetSelection), String> {
    let mut context =
        crate::programs::starting(settings, Some(request.agent), request.profile.as_ref())?;
    match (&request.permissions, request.agent) {
        (AgentPermissions::Default, _) => {}
        (AgentPermissions::ReadOnly, Agent::Codex) => {
            if let Some(options) = context.codex_mut() {
                options.sandbox_mode = "read-only".into();
            }
        }
        (AgentPermissions::ReadOnly, Agent::Pi) => {
            if let Some(options) = context.pi_mut() {
                options.tools = adroit_agents::pi::READ_ONLY_TOOLS
                    .map(String::from)
                    .to_vec();
            }
        }
        (AgentPermissions::ReadOnly, Agent::OpenCode) => {
            adroit_agents::opencode::read_only(&mut context);
        }
        (AgentPermissions::ReadOnly, Agent::Claude) => {
            if let Some(options) = context.claude_mut() {
                options.read_only = Some(
                    adroit_agents::claude::ReadOnly::for_dir(&request.cwd)
                        .protected()
                        .to_vec(),
                );
            }
        }
        (AgentPermissions::Custom(profile), agent) => match (profile.as_ref(), agent) {
            (
                CustomPermissions::Claude {
                    permission_mode,
                    disallowed_tools,
                },
                Agent::Claude,
            ) => {
                if let Some(options) = context.claude_mut() {
                    options.permission_mode = permission_mode.clone();
                    options.disallowed_tools = disallowed_tools.clone();
                }
            }
            (
                CustomPermissions::Codex {
                    approval_policy,
                    sandbox_mode,
                },
                Agent::Codex,
            ) => {
                if let Some(options) = context.codex_mut() {
                    options.approval_policy = approval_policy.clone();
                    options.sandbox_mode = sandbox_mode.clone();
                }
            }
            (CustomPermissions::Pi { tools }, Agent::Pi) => {
                if let Some(options) = context.pi_mut() {
                    options.tools = tools.clone();
                }
            }
            (
                CustomPermissions::OpenCode {
                    primary,
                    permissions,
                    auto,
                },
                Agent::OpenCode,
            ) => {
                if let Some(options) = context.opencode_mut() {
                    options.agent = primary.clone();
                    options.permissions = permissions.clone();
                    options.auto = *auto;
                }
            }
            _ => return Err("custom permissions belong to another agent".into()),
        },
    }
    let model = context.options.model_mut();
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
    let target = adroit_model::TargetSelection {
        agent: request.agent,
        requested_model: request.model.clone(),
        model,
        model_source: source,
        permissions: request.permissions.clone(),
    };
    Ok((context, target))
}

pub fn select_target(
    settings: &Settings,
    agent: Agent,
    model: Option<String>,
    permissions: AgentPermissions,
) -> Result<adroit_model::TargetSelection, String> {
    configured(
        settings,
        &StartRequest {
            agent,
            model,
            permissions,
            ..Default::default()
        },
    )
    .map(|(_, target)| target)
}

pub use adroit_agents::{DeliveryFailure, DeliveryFuture};

/// Submit to a native queue without touching the TUI's input buffer.
pub fn deliver(
    context: adroit_agents::AgentContext,
    cwd: PathBuf,
    conversation: String,
    envelope: String,
) -> DeliveryFuture {
    agents::adapter(context.agent()).deliver(context, cwd, conversation, envelope)
}
