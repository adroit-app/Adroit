//! How the programs adroit runs are started as the settings say: the
//! account's login shell, and the agents' launches. What the settings say of
//! each is `adroit_model::config`.

use adroit_agents::AgentContext;
use adroit_model::Agent;

use crate::settings::Settings;

/// The account's login shell, or `/bin/sh` when it cannot be told.
pub fn login_shell() -> String {
    crate::env::login_shell().unwrap_or_else(|| "/bin/sh".into())
}

/// How `agent` starts as the settings say now: as its default profile says.
pub fn context(agent: Agent) -> AgentContext {
    context_of(&crate::settings::current(), agent)
}

/// How `agent` starts as `settings` say: as its default profile says, in
/// the connection that profile names.
pub fn context_of(settings: &Settings, agent: Agent) -> AgentContext {
    let profile = settings.profile_of(agent);
    let home = settings
        .connections
        .iter()
        .find(|c| c.id == profile.connection)
        .map(|c| c.home.clone());
    AgentContext::new(&profile, home, settings.shell.program(&login_shell()))
}
