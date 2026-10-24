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
/// not defined or starts another agent.
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
    in_login(settings, &chosen)
}

/// Whether a session of `context` can be started at all, in `cwd` with
/// `env` set: its login's state directory made or there, and its
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
        && let Err(e) = adroit_agents::own_login::make_private(home)
    {
        return Err(format!(
            "profile `{}` keeps its login in {}, which cannot be made: {e}; check that its volume is there, or choose your terminal's login for it under {WHERE}",
            context.profile,
            home.display()
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

/// The context a session recorded as started from `profile` in the folder
/// `home`, none for its agent's own environment, runs in, as `launch`, the
/// settings it was started with, say: in that folder, whichever profile has
/// it since; why not, when they hold no such profile.
pub fn recorded(
    launch: &Settings,
    profile: &ProfileId,
    home: Option<&std::path::Path>,
) -> Result<AgentContext, String> {
    let profile = launch.find_profile(profile).ok_or_else(|| {
        format!("the session's profile `{profile}` is not in the settings it was started with")
    })?;
    in_login(
        launch,
        &AgentProfile {
            home: home.map(ToOwned::to_owned),
            ..profile.into_owned()
        },
    )
}

/// Each profile a new session can start from, as `settings` say, in its
/// login: the profiles, and the one adroit comes with for an agent that has
/// none.
pub fn profiles(settings: &Settings) -> Vec<AgentContext> {
    let builtin = Agent::ALL
        .iter()
        .map(|a| settings.profile_of(*a))
        .filter(|p| matches!(p, Cow::Owned(_)))
        .map(Cow::into_owned);
    let open: Vec<AgentProfile> = settings.profiles.iter().cloned().chain(builtin).collect();
    open.iter()
        .filter_map(|p| in_login(settings, p).ok())
        .collect()
}

/// One context for each login a profile runs in, as
/// `settings` say, asked through the agent's default profile where that runs
/// there, else through the first profile that does: what is kept in the
/// logins a new session can run in.
pub fn logins(settings: &Settings) -> Vec<AgentContext> {
    let defaults: Vec<Cow<'_, AgentProfile>> =
        Agent::ALL.iter().map(|a| settings.profile_of(*a)).collect();
    let others = settings.profiles.iter();
    let mut out: Vec<AgentContext> = Vec::new();
    for profile in defaults.iter().map(|p| p.as_ref()).chain(others) {
        let taken = out
            .iter()
            .any(|c| c.agent() == profile.agent() && c.home == profile.home);
        if !taken && let Ok(context) = in_login(settings, profile) {
            out.push(context);
        }
    }
    out
}

/// The context the profile `id` is logged in and asked about in, as
/// `settings` say.
pub fn for_profile(settings: &Settings, id: &ProfileId) -> Result<AgentContext, String> {
    let profile = settings
        .find_profile(id)
        .ok_or_else(|| format!("profile `{id}` is not defined"))?;
    in_login(settings, &profile)
}

/// The context `session` runs in, as it was started: from its record's
/// settings, in the folder it ran in, whatever the settings say now. A
/// session without a record ran in its agent's own environment; one that
/// names a folder of its own but has no record is refused rather than taken
/// to any login.
pub async fn of_session(session: &adroit_model::Session) -> Result<AgentContext, String> {
    static KNOWN: LazyLock<Mutex<HashMap<SessionId, AgentContext>>> =
        LazyLock::new(Default::default);
    let Some(record) = session.delegation else {
        return match &session.home {
            None => Ok(AgentContext::new(
                &AgentProfile::builtin(session.agent),
                crate::settings::current().shell.program(&login_shell()),
            )),
            Some(home) => Err(format!(
                "session {} keeps its login in {}, which no record of it names",
                session.name,
                home.display()
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
    let context = recorded(&launch, &session.profile, session.home.as_deref())?;
    // A record never changes, so neither does what it says.
    KNOWN
        .lock()
        .expect("the contexts' lock")
        .insert(record, context.clone());
    Ok(context)
}

/// `profile` run in its login, as `settings` start its programs.
fn in_login(settings: &Settings, profile: &AgentProfile) -> Result<AgentContext, String> {
    let context = AgentContext::new(profile, settings.shell.program(&login_shell()));
    match adroit_agents::conflict(adroit_agents::adapter(profile.agent()), &context) {
        Some(name) => Err(format!(
            "profile `{}` cannot set `{name}` in its launch: its own login keeps its credentials and state directory apart",
            profile.id
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
name = "Claude Work"
agent = "claude"
home = "/Users/me/claude-work"

[[profiles]]
id = "codex-review"
agent = "codex"
"#;

    fn settings() -> Settings {
        Settings::parse(PROFILES).unwrap()
    }

    fn id(text: &str) -> ProfileId {
        text.parse().unwrap()
    }

    #[test]
    fn a_named_profile_starts_in_its_own_login_s_directory() {
        let context = starting(&settings(), None, Some(&id("claude-work"))).unwrap();
        assert_eq!(context.profile, id("claude-work"));
        assert_eq!(context.home, Some("/Users/me/claude-work".into()));
        let own = starting(&settings(), Some(Agent::Claude), Some(&id("claude"))).unwrap();
        assert_eq!(own.home, None);
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
    fn a_profile_that_is_not_defined_or_starts_another_agent_is_refused() {
        let s = settings();
        let refused = |agent, profile: &str| starting(&s, agent, Some(&id(profile))).unwrap_err();
        assert!(refused(None, "gone").starts_with("profile `gone` is not defined; choose another"));
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
        assert_eq!((named.agent(), named.home), (Agent::Codex, None));
        assert_eq!(
            recorded(&only_claude, &id("codex"), None).unwrap().profile,
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
    fn a_profile_with_a_login_of_its_own_cannot_set_what_the_login_decides_on() {
        for (agent, name) in [
            ("claude", "CLAUDE_CONFIG_DIR"),
            ("claude", "ANTHROPIC_API_KEY"),
            ("codex", "CODEX_HOME"),
            ("codex", "OPENAI_API_KEY"),
            ("pi", "PI_CODING_AGENT_DIR"),
            ("pi", "AWS_PROFILE"),
            ("opencode", "XDG_DATA_HOME"),
            ("opencode", "OPENCODE_AUTH_CONTENT"),
        ] {
            let text = format!(
                "[[profiles]]\nid = \"p\"\nagent = \"{agent}\"\nhome = \"/w\"\n[profiles.launch.env]\n{name} = \"x\"\n"
            );
            let s = Settings::parse(&text).unwrap();
            assert_eq!(
                starting(&s, None, Some(&id("p"))).unwrap_err(),
                format!(
                    "profile `p` cannot set `{name}` in its launch: its own login keeps its credentials and state directory apart"
                )
            );
            // In the agent's own environment the launch sets what it likes.
            let own = Settings::parse(&text.replace("home = \"/w\"\n", "")).unwrap();
            assert!(starting(&own, None, Some(&id("p"))).is_ok());
        }
    }

    #[test]
    fn every_profile_a_session_can_start_from_is_listed_once_in_its_login() {
        let listed: Vec<String> = profiles(&settings())
            .iter()
            .map(|c| c.profile.to_string())
            .collect();
        assert_eq!(
            listed,
            ["claude", "claude-work", "codex-review", "pi", "opencode"]
        );
        let only_claude =
            Settings::parse("[[profiles]]\nid = \"mine\"\nagent = \"claude\"\n").unwrap();
        let listed: Vec<String> = profiles(&only_claude)
            .iter()
            .map(|c| c.profile.to_string())
            .collect();
        assert_eq!(listed, ["mine", "codex", "pi", "opencode"]);
    }

    #[test]
    fn each_login_is_asked_once_through_its_agent_s_default_profile_where_it_runs_there() {
        let mut s = settings();
        // Two profiles in Codex's own environment are one login.
        s.profiles.push(AgentProfile {
            id: id("codex-other"),
            ..AgentProfile::builtin(Agent::Codex)
        });
        let listed: Vec<(String, Option<std::path::PathBuf>)> = logins(&s)
            .iter()
            .map(|c| (c.profile.to_string(), c.home.clone()))
            .collect();
        assert_eq!(
            listed,
            [
                ("claude-work".into(), Some("/Users/me/claude-work".into())),
                ("codex-review".into(), None),
                ("pi".into(), None),
                ("opencode".into(), None),
                ("claude".into(), None),
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
            error.contains("choose your terminal's login for it under Settings → Agents"),
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
    fn a_profile_s_login_is_asked_about_through_the_profile_with_its_own_launch() {
        let mut s = settings();
        s.profiles[1].launch.command = vec!["/opt/claude".into()];
        let work = for_profile(&s, &id("claude-work")).unwrap();
        assert_eq!(work.home, Some("/Users/me/claude-work".into()));
        assert_eq!(work.launch.command, ["/opt/claude"]);
        // Adroit's own profile of an agent the settings give none.
        assert_eq!(for_profile(&s, &id("pi")).unwrap().agent(), Agent::Pi);
        assert_eq!(
            for_profile(&s, &id("gone")).unwrap_err(),
            "profile `gone` is not defined"
        );
    }

    #[test]
    fn a_recorded_session_runs_in_its_own_profile_as_the_settings_it_started_with_say() {
        let s = settings();
        let work = recorded(
            &s,
            &id("claude-work"),
            Some(Path::new("/Users/me/claude-work")),
        )
        .unwrap();
        assert_eq!(work.home, Some("/Users/me/claude-work".into()));
        // A session keeps the folder it ran in, whichever profile has it since.
        let moved = recorded(&s, &id("claude"), Some(Path::new("/Users/me/shared"))).unwrap();
        assert_eq!(moved.home, Some("/Users/me/shared".into()));
        assert_eq!(recorded(&s, &id("claude-work"), None).unwrap().home, None);
        assert!(
            recorded(&Settings::default(), &id("claude-work"), None)
                .unwrap_err()
                .contains("`claude-work` is not in the settings it was started with")
        );
    }

    #[test]
    fn a_recorded_session_runs_and_is_named_as_it_started_after_its_profile_is_deleted() {
        let started_with = settings();
        let mut now = started_with.clone();
        now.profile = id("claude");
        adroit_core::profiles::delete_profile(&mut now, &id("claude-work")).unwrap();
        assert!(for_profile(&now, &id("claude-work")).is_err());
        let work = recorded(
            &started_with,
            &id("claude-work"),
            Some(Path::new("/Users/me/claude-work")),
        )
        .unwrap();
        assert_eq!(work.home, Some("/Users/me/claude-work".into()));
        assert_eq!(work.profile_name, "Claude Work");
        // Nor does a profile given its id since stand in for it.
        adroit_core::profiles::add_profile(&mut now, "Claude Work", Agent::Codex).unwrap();
        assert_eq!(
            recorded(
                &started_with,
                &id("claude-work"),
                Some(Path::new("/Users/me/claude-work"))
            )
            .unwrap()
            .agent(),
            Agent::Claude
        );
    }

    #[test]
    fn every_agent_starts_from_the_profile_offered_whatever_settings_delete_or_move() {
        use adroit_core::profiles::{change_agent, delete_profile};
        let s = settings();
        let mut deleted = s.clone();
        delete_profile(&mut deleted, &id("codex-review")).unwrap();
        let mut moved = s.clone();
        moved.profile = id("claude");
        delete_profile(&mut moved, &id("claude-work")).unwrap();
        change_agent(&mut moved, &id("claude"), Agent::Codex).unwrap();
        // Codex's key itself held by a profile of another agent, too.
        let keyed = Settings::parse(
            "[[profiles]]\nid = \"claude\"\nagent = \"claude\"\n[[profiles]]\nid = \"codex\"\nagent = \"claude\"\n",
        )
        .unwrap();
        for s in [deleted, moved, keyed] {
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
