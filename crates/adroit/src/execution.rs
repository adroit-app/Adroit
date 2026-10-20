//! Launch preparation shared by the window, commands and control service.

use std::path::{Path, PathBuf};

use adroit_model::Agent;
use adroit_term::Spawn;

use crate::{agents, sessions, settings::Settings};

#[derive(Debug, Clone, Default)]
pub struct StartRequest {
    pub agent: Agent,
    pub model: Option<String>,
    pub name: String,
    pub cwd: PathBuf,
    pub prompt: String,
    pub images: Vec<PathBuf>,
    pub read_only: bool,
    pub env: Vec<(String, String)>,
    pub started_ms: u64,
    pub started_from: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ModelSource {
    Explicit,
    Settings,
    AgentDefault,
}

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

pub fn resolve(
    settings: &Settings,
    request: StartRequest,
    adroit_program: &Path,
) -> Result<ResolvedLaunch, String> {
    let mut settings = settings.clone();
    let model = match (request.agent, request.read_only) {
        (Agent::Claude, false) => &mut settings.sessions.model,
        (Agent::Claude, true) => &mut settings.readers.model,
        (Agent::Codex, false) => &mut settings.codex_sessions.model,
        (Agent::Codex, true) => &mut settings.codex_readers.model,
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
    args.extend(adapter.image_args(&request.images));
    args.extend(adapter.start_args(
        &settings,
        &request.name,
        &conversation,
        request.read_only,
        &adapter.image_prompt(&request.prompt, &request.images),
    ));
    let (program, args, env) =
        agents::launch_with_settings(&settings, request.agent, args, request.env);
    let tags = sessions::agent_tags(
        request.agent,
        &request.name,
        &request.cwd,
        &conversation,
        request.read_only,
        request.started_ms,
        request.started_from.as_deref(),
    );
    Ok(ResolvedLaunch {
        agent: request.agent,
        requested_model: request.model,
        model,
        model_source: source,
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
