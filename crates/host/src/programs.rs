//! How the programs adroit runs are started as the settings say: the
//! account's login shell, and the agents' launches. What the settings say of
//! each is `adroit_model::config`.

use std::borrow::Cow;
use std::collections::HashMap;
use std::sync::{LazyLock, Mutex};

use adroit_agents::AgentContext;
use adroit_model::profile::{AgentProfile, ConnectionId, ProfileId};
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
                .ok_or_else(|| {
                    format!(
                        "profile `{id}` is not defined; choose another, or add it under Settings → Agents"
                    )
                })?;
            if profile.archived {
                return Err(format!(
                    "profile `{id}` is archived and starts no new session; choose another, or unarchive it under Settings → Agents → Archived"
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

/// Whether a session of `context` can be started at all, in `cwd` with
/// `env` set: its connection's state directory made or there, and its
/// program found, as the terminal host would find it with the profile's own
/// variables set over `env`, when it is started without a shell, whose
/// startup files could still put it on the path. Why not, with what to do
/// about it.
pub fn ready(
    context: &AgentContext,
    cwd: &std::path::Path,
    env: &[(String, String)],
) -> Result<(), String> {
    const WHERE: &str = "Settings → Agents";
    if let Some(home) = &context.home
        && let Err(e) = adroit_agents::connection::make_private(home)
    {
        return Err(format!(
            "account `{}` keeps its state in {}, which cannot be made: {e}; check that its volume is there, or choose another account for profile `{}` under {WHERE}",
            context.connection,
            home.display(),
            context.profile
        ));
    }
    if context.launch.via_shell {
        return Ok(());
    }
    let agent = context.agent();
    let program = context
        .launch
        .command
        .first()
        .cloned()
        .unwrap_or_else(|| adroit_agents::adapter(agent).program());
    let env: Vec<(String, String)> = env
        .iter()
        .cloned()
        .chain(context.launch.env.clone())
        .collect();
    match adroit_term::runnable(&program, cwd, &env) {
        true => Ok(()),
        false => Err(format!(
            "{}'s program `{program}` is not installed or not on the path; install it, or set profile `{}`'s launch command under {WHERE}",
            agent.label(),
            context.profile
        )),
    }
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

/// The context the connection `id` is logged in and asked about in, as
/// `settings` say: through `profile` when one is named, which has to run
/// there, so the CLI is the one that profile starts; else through its
/// agent's default profile when that runs there, else the first profile
/// that does, archived or not, else adroit's own profile of the agent.
pub fn for_connection(
    settings: &Settings,
    id: &ConnectionId,
    profile: Option<&ProfileId>,
) -> Result<AgentContext, String> {
    if let Some(named) = profile {
        let found = settings
            .find_profile(named)
            .ok_or_else(|| format!("profile `{named}` is not defined"))?;
        if found.connection != *id {
            return Err(format!(
                "profile `{named}` runs in connection `{}`, not `{id}`",
                found.connection
            ));
        }
        return in_connection(settings, &found);
    }
    let agent = match id.implicit_agent() {
        Some(agent) => agent,
        None => {
            settings
                .connections
                .iter()
                .find(|c| &c.id == id)
                .ok_or_else(|| format!("connection `{id}` is not defined"))?
                .agent
        }
    };
    let default = settings.profile_of(agent);
    let profile = match default.connection == *id {
        true => default.into_owned(),
        false => {
            let open = settings
                .profiles
                .iter()
                .filter(|p| &p.connection == id && p.agent() == agent);
            let (open, archived): (Vec<_>, Vec<_>) = open.partition(|p| !p.archived);
            match open.into_iter().chain(archived).next() {
                Some(p) => p.clone(),
                None => AgentProfile {
                    connection: id.clone(),
                    ..AgentProfile::builtin(agent)
                },
            }
        }
    };
    in_connection(settings, &profile)
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
    use std::path::Path;

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
        assert!(refused(None, "gone").starts_with("profile `gone` is not defined; choose another"));
        assert!(refused(None, "old").ends_with("unarchive it under Settings → Agents → Archived"));
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
        assert!(
            starting(&settings(), None, Some(&id("codex")))
                .unwrap_err()
                .starts_with("profile `codex` is not defined")
        );
    }

    #[test]
    fn a_profile_in_a_connection_of_its_own_cannot_set_what_the_connection_decides_on() {
        for (agent, name) in [
            ("claude", "CLAUDE_CONFIG_DIR"),
            ("claude", "ANTHROPIC_API_KEY"),
            ("codex", "CODEX_HOME"),
            ("codex", "OPENAI_API_KEY"),
            ("pi", "PI_CODING_AGENT_DIR"),
            ("pi", "AWS_PROFILE"),
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
        assert_eq!(listed, ["claude", "claude-work", "codex-review", "pi"]);
        let only_claude =
            Settings::parse("[[profiles]]\nid = \"mine\"\nagent = \"claude\"\n").unwrap();
        let listed: Vec<String> = profiles(&only_claude)
            .iter()
            .map(|c| c.profile.to_string())
            .collect();
        assert_eq!(listed, ["mine", "codex", "pi"]);
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
                ("pi".into(), "pi".into()),
                ("claude".into(), "claude".into()),
            ]
        );
    }

    #[test]
    fn a_start_whose_program_cannot_be_found_or_state_made_says_what_to_do() {
        use std::os::unix::fs::PermissionsExt;
        let dir = tempfile::tempdir().unwrap();
        let bin = dir.path().join("bin");
        std::fs::create_dir(&bin).unwrap();
        let program = |path: &Path, mode| {
            std::fs::write(path, "").unwrap();
            std::fs::set_permissions(path, std::fs::Permissions::from_mode(mode)).unwrap();
        };
        program(&bin.join("claude"), 0o755);
        let cwd = dir.path();
        let path = vec![("PATH".to_string(), format!("/nowhere:{}", bin.display()))];
        let own = AgentContext::builtin(Agent::Claude);
        assert_eq!(ready(&own, cwd, &path), Ok(()));
        let codex = AgentContext::builtin(Agent::Codex);
        let missing = ready(&codex, cwd, &path).unwrap_err();
        assert!(
            missing.starts_with("Codex's program `codex` is not installed or not on the path; install it, or set profile `codex`'s launch command under Settings"),
            "{missing}"
        );
        // A shell's startup files may still put it on the path.
        let mut shelled = codex.clone();
        shelled.launch.via_shell = true;
        assert_eq!(ready(&shelled, cwd, &path), Ok(()));
        let mut named = codex.clone();
        named.launch.command = vec![bin.join("claude").display().to_string()];
        assert_eq!(ready(&named, cwd, &[]), Ok(()));
        named.launch.command = vec!["/nowhere/codex".into()];
        assert!(ready(&named, cwd, &[]).is_err());
        // A command with a `/` is taken from the session's directory.
        named.launch.command = vec!["bin/claude".into()];
        assert_eq!(ready(&named, cwd, &[]), Ok(()));
        assert!(ready(&named, &bin, &[]).is_err());
        // A file that cannot be run is no program.
        program(&bin.join("codex"), 0o644);
        assert!(ready(&codex, cwd, &path).is_err());

        let home = dir.path().join("work");
        let mut in_work = AgentContext {
            home: Some(home.clone()),
            ..own.clone()
        };
        assert_eq!(ready(&in_work, cwd, &path), Ok(()));
        assert!(home.is_dir());
        // A file where the directory would go keeps it from being made.
        let blocked = dir.path().join("file");
        std::fs::write(&blocked, "").unwrap();
        in_work.home = Some(blocked.join("work"));
        let error = ready(&in_work, cwd, &path).unwrap_err();
        assert!(error.contains("which cannot be made"), "{error}");
        assert!(
            error.contains("choose another account for profile `claude` under Settings → Agents"),
            "{error}"
        );
        assert!(!error.contains("→ Profiles"), "{error}");
    }

    #[test]
    fn a_program_is_looked_up_on_the_path_its_profile_sets_before_the_one_it_is_started_with() {
        use std::os::unix::fs::PermissionsExt;
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("codex"), "").unwrap();
        std::fs::set_permissions(
            dir.path().join("codex"),
            std::fs::Permissions::from_mode(0o755),
        )
        .unwrap();
        let mut codex = AgentContext::builtin(Agent::Codex);
        codex
            .launch
            .env
            .insert("PATH".into(), dir.path().display().to_string());
        let elsewhere = [("PATH".to_string(), "/nowhere".to_string())];
        assert_eq!(ready(&codex, Path::new("/"), &elsewhere), Ok(()));
    }

    #[test]
    fn a_connection_is_asked_about_through_a_profile_that_runs_there() {
        let s = settings();
        let cid = |text: &str| text.parse::<ConnectionId>().unwrap();
        let work = for_connection(&s, &cid("work"), None).unwrap();
        assert_eq!(work.profile, id("claude-work"));
        assert_eq!(work.home, Some("/Users/me/claude-work".into()));
        let own = for_connection(&s, &cid("claude"), None).unwrap();
        assert_eq!((own.profile, own.home), (id("claude"), None));
        let codex = for_connection(&s, &cid("codex"), None).unwrap();
        assert_eq!(codex.profile, id("codex-review"));
        // A connection no profile runs in is asked as adroit's own profile.
        let mut unused = settings();
        unused.profiles.retain(|p| p.connection.as_str() != "work");
        unused.profile = id("claude");
        let alone = for_connection(&unused, &cid("work"), None).unwrap();
        assert_eq!(alone.profile, id("claude"));
        assert_eq!(alone.connection, cid("work"));
        assert_eq!(alone.home, Some("/Users/me/claude-work".into()));
        assert_eq!(
            for_connection(&s, &cid("gone"), None).unwrap_err(),
            "connection `gone` is not defined"
        );
    }

    #[test]
    fn a_connection_is_asked_about_through_the_profile_named_with_its_own_launch() {
        let mut s = settings();
        let mut copy = s.profiles[1].clone();
        copy.id = id("claude-work-cli");
        copy.launch.command = vec!["/opt/claude".into()];
        copy.launch.via_shell = true;
        s.profiles.push(copy);
        let cid = |text: &str| text.parse::<ConnectionId>().unwrap();
        let named = for_connection(&s, &cid("work"), Some(&id("claude-work-cli"))).unwrap();
        assert_eq!(named.profile, id("claude-work-cli"));
        assert_eq!(named.launch.command, ["/opt/claude"]);
        assert!(named.launch.via_shell);
        assert_eq!(named.home, Some("/Users/me/claude-work".into()));
        assert_eq!(
            for_connection(&s, &cid("claude"), Some(&id("claude-work-cli"))).unwrap_err(),
            "profile `claude-work-cli` runs in connection `work`, not `claude`"
        );
        assert_eq!(
            for_connection(&s, &cid("work"), Some(&id("gone"))).unwrap_err(),
            "profile `gone` is not defined"
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
