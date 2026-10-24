//! How the programs adroit runs are started as the settings say: the
//! account's login shell, and the agents' launches. What the settings say of
//! each is `adroit_model::config`.

/// The account's login shell, or `/bin/sh` when it cannot be told.
pub fn login_shell() -> String {
    crate::env::login_shell().unwrap_or_else(|| "/bin/sh".into())
}

/// What the settings say now of how agents start.
pub fn agents() -> adroit_agents::AgentSettings {
    agents_of(&crate::settings::current())
}

/// What `settings` say of how agents start: as each agent's default profile
/// says.
pub fn agents_of(settings: &crate::settings::Settings) -> adroit_agents::AgentSettings {
    use adroit_model::Agent;
    let claude = settings.profile_of(Agent::Claude);
    let codex = settings.profile_of(Agent::Codex);
    adroit_agents::AgentSettings {
        shell: settings.shell.program(&login_shell()),
        claude: claude.launch.clone(),
        codex: codex.launch.clone(),
        sessions: claude.claude().cloned().unwrap_or_default(),
        codex_sessions: codex.codex().cloned().unwrap_or_default(),
    }
}

/// What starts `agent` with `args`, as the settings say now: its launch
/// command, and `env` with its environment.
pub fn launch(
    agent: adroit_model::Agent,
    args: Vec<String>,
    env: Vec<(String, String)>,
) -> adroit_agents::Started {
    adroit_agents::launch_with_settings(&agents(), agent, args, env)
}
