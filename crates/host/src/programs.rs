//! How the programs adroit runs are started as the settings say: the
//! account's login shell, and the agents' launches. What the settings say of
//! each is `adroit_model::config`.

use std::borrow::Cow;
use std::collections::HashMap;
use std::sync::{LazyLock, Mutex};

use adroit_agents::AgentContext;
use adroit_model::profile::{AgentProfile, ProfileId};
use adroit_model::{Agent, SessionId};

use crate::settings::Settings;

/// The account's login shell, or `/bin/sh` when it cannot be told.
pub fn login_shell() -> String {
    crate::env::login_shell().unwrap_or_else(|| "/bin/sh".into())
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

/// Each profile a new session can start from, as `settings` say, in its
/// connection: those not archived, and the one adroit comes with for an agent
/// that has none.
pub fn profiles(settings: &Settings) -> Vec<AgentContext> {
    let builtin = Agent::ALL
        .iter()
        .map(|a| settings.profile_of(*a))
        .filter(|p| matches!(p, Cow::Owned(_)))
        .map(Cow::into_owned);
    let open: Vec<AgentProfile> = settings
        .profiles
        .iter()
        .filter(|p| !p.archived)
        .cloned()
        .chain(builtin)
        .collect();
    open.iter()
        .filter_map(|p| in_connection(settings, p).ok())
        .collect()
}

/// One context for each connection a profile not archived runs in, as
/// `settings` say, asked through the agent's default profile where that runs
/// there, else through the first profile that does: what is kept in the
/// connections a new session can run in.
pub fn connections(settings: &Settings) -> Vec<AgentContext> {
    let defaults: Vec<Cow<'_, AgentProfile>> =
        Agent::ALL.iter().map(|a| settings.profile_of(*a)).collect();
    let others = settings.profiles.iter().filter(|p| !p.archived);
    let mut out: Vec<AgentContext> = Vec::new();
    for profile in defaults.iter().map(|p| p.as_ref()).chain(others) {
        let taken = out
            .iter()
            .any(|c| c.agent() == profile.agent() && c.connection == profile.connection);
        if !taken && let Ok(context) = in_connection(settings, profile) {
            out.push(context);
        }
    }
    out
}

/// The context `session` runs in, as it was started: from its record's
/// settings, whatever the settings say now. A session without a record ran
/// in its agent's own environment; one that names a connection of its own
/// but has no record is refused rather than taken to any login.
pub async fn of_session(session: &adroit_model::Session) -> Result<AgentContext, String> {
    static KNOWN: LazyLock<Mutex<HashMap<SessionId, AgentContext>>> =
        LazyLock::new(Default::default);
    let Some(record) = session.delegation else {
        return match session.connection.implicit_agent() {
            Some(agent) if agent == session.agent => Ok(AgentContext::new(
                &AgentProfile::builtin(agent),
                None,
                crate::settings::current().shell.program(&login_shell()),
            )),
            _ => Err(format!(
                "session {} runs in connection `{}`, which no record of it names",
                session.name, session.connection
            )),
        };
    };
    if let Some(known) = KNOWN.lock().expect("the contexts' lock").get(&record) {
        return Ok(known.clone());
    }
    let launch = crate::delegation::app::launch_of(&crate::delegation::app::root(), record)
        .await
        .map_err(|error| error.to_string())?
        .ok_or_else(|| format!("session {} has no record of how it started", session.name))?;
    let context = recorded(&launch, &session.profile)?;
    // A record never changes, so neither does what it says.
    KNOWN
        .lock()
        .expect("the contexts' lock")
        .insert(record, context.clone());
    Ok(context)
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
    let context = AgentContext::new(profile, home, settings.shell.program(&login_shell()));
    match adroit_agents::conflict(adroit_agents::adapter(profile.agent()), &context) {
        Some(name) => Err(format!(
            "profile `{}` cannot set `{name}` in its launch: connection `{}` keeps its own login and state directory",
            profile.id, profile.connection
        )),
        None => Ok(context),
    }
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
    fn a_profile_in_a_connection_of_its_own_cannot_set_what_the_connection_decides_on() {
        for (agent, name) in [
            ("claude", "CLAUDE_CONFIG_DIR"),
            ("claude", "ANTHROPIC_API_KEY"),
            ("codex", "CODEX_HOME"),
            ("codex", "OPENAI_API_KEY"),
        ] {
            let text = format!(
                "[[profiles]]\nid = \"p\"\nagent = \"{agent}\"\nconnection = \"work\"\n[profiles.launch.env]\n{name} = \"x\"\n\n[[connections]]\nid = \"work\"\nagent = \"{agent}\"\nhome = \"/w\"\n"
            );
            let s = Settings::parse(&text).unwrap();
            assert_eq!(
                starting(&s, None, Some(&id("p"))).unwrap_err(),
                format!(
                    "profile `p` cannot set `{name}` in its launch: connection `work` keeps its own login and state directory"
                )
            );
            // In the agent's own environment the launch sets what it likes.
            let own = Settings::parse(&text.replace("connection = \"work\"\n", "")).unwrap();
            assert!(starting(&own, None, Some(&id("p"))).is_ok());
        }
    }

    #[test]
    fn every_profile_a_session_can_start_from_is_listed_once_in_its_connection() {
        let listed: Vec<String> = profiles(&settings())
            .iter()
            .map(|c| c.profile.to_string())
            .collect();
        assert_eq!(listed, ["claude", "claude-work", "codex-review"]);
        let only_claude =
            Settings::parse("[[profiles]]\nid = \"mine\"\nagent = \"claude\"\n").unwrap();
        let listed: Vec<String> = profiles(&only_claude)
            .iter()
            .map(|c| c.profile.to_string())
            .collect();
        assert_eq!(listed, ["mine", "codex"]);
    }

    #[test]
    fn each_connection_is_asked_once_through_its_agent_s_default_profile_where_it_runs_there() {
        let listed: Vec<(String, String)> = connections(&settings())
            .iter()
            .map(|c| (c.profile.to_string(), c.connection.to_string()))
            .collect();
        assert_eq!(
            listed,
            [
                ("claude-work".into(), "work".into()),
                ("codex-review".into(), "codex".into()),
                ("claude".into(), "claude".into()),
            ]
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

    #[test]
    fn every_agent_starts_from_the_profile_offered_whatever_settings_archive_or_move() {
        use adroit_core::profiles::{archive_profile, change_agent};
        let s = settings();
        let mut archived = s.clone();
        archive_profile(&mut archived, &id("codex-review")).unwrap();
        archive_profile(&mut archived, &id("old")).unwrap();
        let mut moved = s.clone();
        moved.profile = id("claude");
        archive_profile(&mut moved, &id("claude-work")).unwrap();
        change_agent(&mut moved, &id("claude"), Agent::Codex).unwrap();
        // Codex's key itself held by an archived profile, too.
        let mut keyed = Settings::parse(
            "[[profiles]]\nid = \"claude\"\nagent = \"claude\"\n[[profiles]]\nid = \"codex\"\nagent = \"codex\"\n",
        )
        .unwrap();
        archive_profile(&mut keyed, &id("codex")).unwrap();
        for s in [archived, moved, keyed] {
            for agent in Agent::ALL {
                let offered = s.profile_of(agent).id.clone();
                let started = starting(&s, Some(agent), Some(&offered));
                assert_eq!(
                    started.map(|c| (c.agent(), c.profile)),
                    Ok((agent, offered))
                );
            }
        }
    }
}
