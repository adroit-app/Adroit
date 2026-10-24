//! How the programs adroit runs are started as the settings say: the
//! account's login shell, and the agents' launches. What the settings say of
//! each is `adroit_model::config`.

use adroit_agents::AgentContext;
use adroit_model::Agent;
use adroit_model::profile::{AgentProfile, ProfileId};

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

/// The context a new session starts in, as `settings` say: `profile` when
/// it is named, which `agent`, when also named, has to start; else `agent`'s
/// default profile; else the default profile. Why not, when the profile is
/// not defined, is archived or starts another agent.
pub fn starting(
    settings: &Settings,
    agent: Option<Agent>,
    profile: Option<&ProfileId>,
) -> Result<AgentContext, String> {
    let chosen = match (profile, agent) {
        (Some(id), agent) => {
            let profile = settings
                .find_profile(id)
                .ok_or_else(|| format!("profile `{id}` is not defined"))?;
            if profile.archived {
                return Err(format!(
                    "profile `{id}` is archived and starts no new session"
                ));
            }
            if let Some(agent) = agent.filter(|a| *a != profile.agent()) {
                return Err(format!(
                    "profile `{id}` starts {}, not {}",
                    profile.agent().label(),
                    agent.label()
                ));
            }
            profile
        }
        (None, Some(agent)) => settings.profile_of(agent),
        (None, None) => settings.profile_of(settings.default_agent()),
    };
    in_connection(settings, &chosen)
}

/// The context a session recorded as started from `profile` runs in, as
/// `launch`, the settings it was started with, say; why not, when they hold
/// no such profile.
pub fn recorded(launch: &Settings, profile: &ProfileId) -> Result<AgentContext, String> {
    let profile = launch.find_profile(profile).ok_or_else(|| {
        format!("the session's profile `{profile}` is not in the settings it was started with")
    })?;
    in_connection(launch, &profile)
}

/// `profile` run in the connection it names, as `settings` define it.
fn in_connection(settings: &Settings, profile: &AgentProfile) -> Result<AgentContext, String> {
    let home = match profile.connection.implicit_agent() {
        Some(_) => None,
        None => Some(
            settings
                .connections
                .iter()
                .find(|c| c.id == profile.connection)
                .ok_or_else(|| {
                    format!(
                        "profile `{}` names connection `{}`, which is not defined",
                        profile.id, profile.connection
                    )
                })?
                .home
                .clone(),
        ),
    };
    Ok(AgentContext::new(
        profile,
        home,
        settings.shell.program(&login_shell()),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    const PROFILES: &str = r#"profile = "claude-work"

[[profiles]]
id = "claude"
agent = "claude"

[[profiles]]
id = "claude-work"
agent = "claude"
connection = "work"

[[profiles]]
id = "codex-review"
agent = "codex"

[[profiles]]
id = "old"
agent = "codex"
archived = true

[[connections]]
id = "work"
agent = "claude"
home = "/Users/me/claude-work"
"#;

    fn settings() -> Settings {
        Settings::parse(PROFILES).unwrap()
    }

    fn id(text: &str) -> ProfileId {
        text.parse().unwrap()
    }

    #[test]
    fn a_named_profile_starts_in_its_connection_s_directory() {
        let context = starting(&settings(), None, Some(&id("claude-work"))).unwrap();
        assert_eq!(context.profile, id("claude-work"));
        assert_eq!(context.connection.as_str(), "work");
        assert_eq!(context.home, Some("/Users/me/claude-work".into()));
        let own = starting(&settings(), Some(Agent::Claude), Some(&id("claude"))).unwrap();
        assert_eq!((own.connection.as_str(), own.home), ("claude", None));
    }

    #[test]
    fn with_no_profile_named_the_agent_s_default_profile_starts_else_the_default_one() {
        let s = settings();
        let pick = |agent| starting(&s, agent, None).unwrap().profile;
        assert_eq!(pick(None), id("claude-work"));
        assert_eq!(pick(Some(Agent::Claude)), id("claude-work"));
        assert_eq!(pick(Some(Agent::Codex)), id("codex-review"));
    }

    #[test]
    fn a_profile_that_is_not_defined_is_archived_or_starts_another_agent_is_refused() {
        let s = settings();
        let refused = |agent, profile: &str| starting(&s, agent, Some(&id(profile))).unwrap_err();
        assert_eq!(refused(None, "gone"), "profile `gone` is not defined");
        assert_eq!(
            refused(None, "old"),
            "profile `old` is archived and starts no new session"
        );
        assert_eq!(
            refused(Some(Agent::Claude), "codex-review"),
            "profile `codex-review` starts Codex, not Claude Code"
        );
    }

    #[test]
    fn adroit_s_own_profile_of_an_agent_the_settings_give_none_is_named_and_recorded_as_any() {
        let only_claude = Settings::parse(
            "[[profiles]]\nid = \"claude\"\nagent = \"claude\"\n\n[[profiles]]\nid = \"claude-work\"\nagent = \"claude\"\n",
        )
        .unwrap();
        let named = starting(&only_claude, Some(Agent::Codex), Some(&id("codex"))).unwrap();
        assert_eq!(
            named.profile,
            starting(&only_claude, Some(Agent::Codex), None)
                .unwrap()
                .profile
        );
        assert_eq!(
            (named.agent(), named.connection.as_str()),
            (Agent::Codex, "codex")
        );
        assert_eq!(
            recorded(&only_claude, &id("codex")).unwrap().profile,
            id("codex")
        );
        // An agent with a profile of its own is started from that alone.
        assert_eq!(
            starting(&settings(), None, Some(&id("codex"))).unwrap_err(),
            "profile `codex` is not defined"
        );
    }

    #[test]
    fn a_recorded_session_runs_in_its_own_profile_even_archived_since() {
        let s = settings();
        assert_eq!(recorded(&s, &id("old")).unwrap().profile, id("old"));
        let work = recorded(&s, &id("claude-work")).unwrap();
        assert_eq!(work.home, Some("/Users/me/claude-work".into()));
        assert!(
            recorded(&Settings::default(), &id("claude-work"))
                .unwrap_err()
                .contains("`claude-work` is not in the settings it was started with")
        );
    }
}
