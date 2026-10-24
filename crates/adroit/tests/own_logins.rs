//! Profiles started in logins of their own, through programs standing
//! in for the agents' that tell what they were started with.

use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::time::Duration;

use adroit_host::execution::{StartRequest, resolve};
use adroit_host::settings::Settings;
use adroit_model::Agent;

/// The adroit program a login of its own starts the agent's program
/// through.
const ADROIT: &str = env!("CARGO_BIN_EXE_adroit");

/// A program standing in for an agent's, which writes to `$ADROIT_TEST_OUT`
/// the variables a login of its own decides on, as it found them, and then its
/// arguments, one a line.
fn agent_program(dir: &Path) -> PathBuf {
    let path = dir.join("agent.sh");
    std::fs::write(
        &path,
        r#"{
printf 'CLAUDE_CONFIG_DIR=%s\n' "${CLAUDE_CONFIG_DIR-unset}"
printf 'CODEX_HOME=%s\n' "${CODEX_HOME-unset}"
printf 'ANTHROPIC_API_KEY=%s\n' "${ANTHROPIC_API_KEY-unset}"
printf 'OPENAI_API_KEY=%s\n' "${OPENAI_API_KEY-unset}"
printf 'CODEX_ACCESS_TOKEN=%s\n' "${CODEX_ACCESS_TOKEN-unset}"
printf 'CLAUDE_CODE_USE_NEWCLOUD=%s\n' "${CLAUDE_CODE_USE_NEWCLOUD-unset}"
printf 'ANTHROPIC_VERTEX_REGION=%s\n' "${ANTHROPIC_VERTEX_REGION-unset}"
printf 'OPENAI_EXTRA=%s\n' "${OPENAI_EXTRA-unset}"
printf 'CODEX_EXTRA=%s\n' "${CODEX_EXTRA-unset}"
printf 'PI_CODING_AGENT_DIR=%s\n' "${PI_CODING_AGENT_DIR-unset}"
printf 'AWS_PROFILE=%s\n' "${AWS_PROFILE-unset}"
printf 'XDG_DATA_HOME=%s\n' "${XDG_DATA_HOME-unset}"
printf 'XDG_CONFIG_HOME=%s\n' "${XDG_CONFIG_HOME-unset}"
printf 'GITHUB_TOKEN=%s\n' "${GITHUB_TOKEN-unset}"
printf 'OPENCODE_DB=%s\n' "${OPENCODE_DB-unset}"
printf 'ADROIT_OUTSIDE_XDG_DATA_HOME=%s\n' "${ADROIT_OUTSIDE_XDG_DATA_HOME-unset}"
printf '%s\n' "$@"
} > "$ADROIT_TEST_OUT"
"#,
    )
    .unwrap();
    path
}

/// A shell whose startup files set again what a login of its own removes, and set
/// its state directory elsewhere, then run what they are handed.
fn meddling_shell(dir: &Path) -> PathBuf {
    let path = dir.join("shell");
    std::fs::write(
        &path,
        "#!/bin/sh\nexport ANTHROPIC_API_KEY=from-rc OPENAI_API_KEY=from-rc CODEX_ACCESS_TOKEN=from-rc\nexport CLAUDE_CODE_USE_NEWCLOUD=from-rc OPENAI_EXTRA=from-rc\nexport CLAUDE_CONFIG_DIR=/from-rc CODEX_HOME=/from-rc PI_CODING_AGENT_DIR=/from-rc\nshift 5\nexec \"$@\"\n",
    )
    .unwrap();
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
    path
}

/// Settings with two profiles of `agent`: `own`, in the agent's own
/// environment, and `work`, with a login of its own in `home`; both run
/// `program`, through `shell` when one is given.
fn settings(agent: Agent, program: &Path, home: &Path, shell: Option<&Path>) -> Settings {
    let key = agent.key();
    let via_shell = shell.is_some();
    let shell = shell.map_or(String::new(), |s| {
        format!("[shell]\ncommand = [\"{}\"]\n\n", s.display())
    });
    let launch = format!(
        "[profiles.launch]\ncommand = [\"/bin/sh\", \"{}\"]\nvia_shell = {via_shell}\n",
        program.display()
    );
    Settings::parse(&format!(
        "profile = \"own\"\n\n{shell}[[profiles]]\nid = \"own\"\nagent = \"{key}\"\n{launch}\n[[profiles]]\nid = \"work\"\nagent = \"{key}\"\nhome = \"{}\"\n{launch}",
        home.display()
    ))
    .unwrap()
}

/// What the program started for `profile` was started with: its variables
/// as it found them, then its arguments, with the variables a login of its own
/// removes inherited from where adroit runs.
fn started(settings: &Settings, agent: Agent, profile: &str, out: &Path) -> Vec<String> {
    let launch = resolve(
        settings,
        StartRequest {
            agent,
            profile: Some(profile.parse().unwrap()),
            name: "fix".into(),
            cwd: "/".into(),
            prompt: "go".into(),
            env: vec![("ADROIT_TEST_OUT".into(), out.display().to_string())],
            ..Default::default()
        },
        Path::new(ADROIT),
    )
    .unwrap();
    let status = std::process::Command::new(&launch.spawn.program)
        .args(&launch.spawn.args)
        .envs(launch.spawn.env.iter().cloned())
        .env("ANTHROPIC_API_KEY", "inherited")
        .env("OPENAI_API_KEY", "inherited")
        .env("CODEX_ACCESS_TOKEN", "inherited")
        .env("ANTHROPIC_VERTEX_REGION", "inherited")
        .env("CODEX_EXTRA", "inherited")
        .env("AWS_PROFILE", "inherited")
        .env("GITHUB_TOKEN", "inherited")
        .env("OPENCODE_DB", "/inherited.db")
        .env("XDG_DATA_HOME", "/outside/data")
        .env_remove("XDG_CONFIG_HOME")
        .env_remove("CLAUDE_CONFIG_DIR")
        .env_remove("CODEX_HOME")
        .env_remove("PI_CODING_AGENT_DIR")
        .status()
        .unwrap();
    assert!(status.success());
    std::fs::read_to_string(out)
        .unwrap()
        .lines()
        .map(String::from)
        .collect()
}

fn has(lines: &[String], line: &str) -> bool {
    lines.iter().any(|l| l == line)
}

#[test]
fn two_claude_code_profiles_start_side_by_side_each_with_its_own_login() {
    let dir = tempfile::tempdir().unwrap();
    let home = dir.path().join("claude-work");
    let s = settings(Agent::Claude, &agent_program(dir.path()), &home, None);

    let own = started(&s, Agent::Claude, "own", &dir.path().join("own"));
    let work = started(&s, Agent::Claude, "work", &dir.path().join("work"));

    // The agent's own environment is left as adroit found it.
    assert!(has(&own, "CLAUDE_CONFIG_DIR=unset"), "{own:?}");
    assert!(has(&own, "ANTHROPIC_API_KEY=inherited"), "{own:?}");
    assert!(
        has(&work, &format!("CLAUDE_CONFIG_DIR={}", home.display())),
        "{work:?}"
    );
    assert!(has(&work, "ANTHROPIC_API_KEY=unset"), "{work:?}");
    assert!(home.is_dir());
}

#[test]
fn two_codex_profiles_start_side_by_side_each_with_its_own_login_and_mcp_tokens() {
    let dir = tempfile::tempdir().unwrap();
    let home = dir.path().join("codex-work");
    let s = settings(Agent::Codex, &agent_program(dir.path()), &home, None);

    let own = started(&s, Agent::Codex, "own", &dir.path().join("own"));
    let work = started(&s, Agent::Codex, "work", &dir.path().join("work"));

    assert!(has(&own, "CODEX_HOME=unset"), "{own:?}");
    assert!(has(&own, "OPENAI_API_KEY=inherited"), "{own:?}");
    assert!(
        !has(&own, "mcp_oauth_credentials_store=\"file\""),
        "{own:?}"
    );
    assert!(
        has(&work, &format!("CODEX_HOME={}", home.display())),
        "{work:?}"
    );
    assert!(has(&work, "OPENAI_API_KEY=unset"), "{work:?}");
    assert!(has(&work, "CODEX_ACCESS_TOKEN=unset"), "{work:?}");
    assert!(
        has(&work, "mcp_oauth_credentials_store=\"file\""),
        "{work:?}"
    );
}

#[test]
fn what_a_shell_s_startup_files_set_again_does_not_reach_a_login_of_its_own() {
    let dir = tempfile::tempdir().unwrap();
    let shell = meddling_shell(dir.path());
    let program = agent_program(dir.path());
    for (agent, state, key) in [
        (Agent::Claude, "CLAUDE_CONFIG_DIR", "ANTHROPIC_API_KEY"),
        (Agent::Codex, "CODEX_HOME", "OPENAI_API_KEY"),
        (Agent::Pi, "PI_CODING_AGENT_DIR", "OPENAI_API_KEY"),
    ] {
        let home = dir.path().join(format!("{}-work", agent.key()));
        let s = settings(agent, &program, &home, Some(&shell));

        let own = started(&s, agent, "own", &dir.path().join("own"));
        let work = started(&s, agent, "work", &dir.path().join("work"));

        // In its own environment the agent gets what the shell sets, as it
        // would started from that shell.
        assert!(has(&own, &format!("{key}=from-rc")), "{own:?}");
        assert!(
            has(&work, &format!("{state}={}", home.display())),
            "{work:?}"
        );
        assert!(has(&work, &format!("{key}=unset")), "{work:?}");
    }
}

#[test]
fn a_profile_whose_launch_sets_its_own_login_s_state_directory_is_refused_naming_it() {
    let dir = tempfile::tempdir().unwrap();
    let mut s = settings(
        Agent::Codex,
        &agent_program(dir.path()),
        &dir.path().join("w"),
        None,
    );
    s.profiles[1]
        .launch
        .env
        .insert("CODEX_HOME".into(), "/elsewhere".into());
    let error = resolve(
        &s,
        StartRequest {
            agent: Agent::Codex,
            profile: Some("work".parse().unwrap()),
            ..Default::default()
        },
        Path::new("/adroit"),
    )
    .unwrap_err();
    assert!(error.contains("cannot set `CODEX_HOME`"), "{error}");
}

#[test]
fn a_variable_of_a_family_a_login_of_its_own_removes_reaches_it_from_neither_the_shell_nor_the_host()
 {
    let dir = tempfile::tempdir().unwrap();
    let shell = meddling_shell(dir.path());
    let program = agent_program(dir.path());
    for (agent, from_rc, inherited) in [
        (
            Agent::Claude,
            "CLAUDE_CODE_USE_NEWCLOUD",
            "ANTHROPIC_VERTEX_REGION",
        ),
        (Agent::Codex, "OPENAI_EXTRA", "CODEX_EXTRA"),
        (Agent::Pi, "OPENAI_EXTRA", "AWS_PROFILE"),
    ] {
        let home = dir.path().join(format!("{}-work", agent.key()));
        let s = settings(agent, &program, &home, Some(&shell));

        let own = started(&s, agent, "own", &dir.path().join("own"));
        let work = started(&s, agent, "work", &dir.path().join("work"));

        assert!(has(&own, &format!("{from_rc}=from-rc")), "{own:?}");
        assert!(has(&own, &format!("{inherited}=inherited")), "{own:?}");
        assert!(has(&work, &format!("{from_rc}=unset")), "{work:?}");
        assert!(has(&work, &format!("{inherited}=unset")), "{work:?}");
    }
}

#[test]
fn a_conversation_resumes_only_in_the_login_that_keeps_it() {
    let dir = tempfile::tempdir().unwrap();
    let home = dir.path().join("w");
    let s = settings(Agent::Codex, &agent_program(dir.path()), &home, None);
    let thread = "019f7a4b-fee9-74d3-9977-5bc9bb134d11";
    let resume = |profile: &str, kept: Option<&Path>| {
        resolve(
            &s,
            StartRequest {
                agent: Agent::Codex,
                profile: Some(profile.parse().unwrap()),
                resume: Some(thread.into()),
                home: kept.map(Path::to_path_buf),
                ..Default::default()
            },
            Path::new("/adroit"),
        )
    };

    let resumed = resume("work", Some(&home)).unwrap();
    let words = resumed.spawn.args.join(" ");
    assert!(
        words.contains(&format!("in-own-login codex {}", home.display())),
        "{words}"
    );
    assert!(words.contains(&format!("resume {thread}")), "{words}");
    let error = resume("own", Some(&home)).unwrap_err();
    assert_eq!(
        error,
        format!(
            "conversation {thread} is kept in {}, which profile `own` does not run in",
            home.display()
        )
    );
    let error = resume("work", None).unwrap_err();
    assert_eq!(
        error,
        format!(
            "conversation {thread} is kept in its agent's own environment, which profile `work` does not run in"
        )
    );
    assert!(
        resume("gone", Some(&home))
            .unwrap_err()
            .contains("profile `gone` is not defined")
    );
}

#[test]
fn an_agent_the_settings_give_no_profile_starts_and_resumes_as_adroit_s_own_profile() {
    let only_claude = Settings::parse(
        "[[profiles]]\nid = \"claude\"\nagent = \"claude\"\n\n[[profiles]]\nid = \"claude-work\"\nagent = \"claude\"\n",
    )
    .unwrap();
    let thread = "019f7a4b-fee9-74d3-9977-5bc9bb134d11";
    let start = |profile: Option<&str>, resume: Option<&str>| {
        resolve(
            &only_claude,
            StartRequest {
                agent: Agent::Codex,
                profile: profile.map(|p| p.parse().unwrap()),
                resume: resume.map(String::from),
                ..Default::default()
            },
            Path::new("/adroit"),
        )
    };

    let started = start(None, None).unwrap();
    let recorded = adroit_host::sessions::profile_tags(&"codex".parse().unwrap(), None);
    assert!(
        recorded.iter().all(|t| started.spawn.tags.contains(t)),
        "{:?}",
        started.spawn.tags
    );
    assert!(start(Some("codex"), None).is_ok());
    let resumed = start(Some("codex"), Some(thread)).unwrap();
    let words = resumed.spawn.args.join(" ");
    assert!(words.contains(&format!("resume {thread}")), "{words}");
}

/// A program standing in for Codex's app server, which lists one model named
/// after the login it was started in: its home's last component.
fn model_listing(dir: &Path) -> PathBuf {
    let path = dir.join("app-server.sh");
    std::fs::write(
        &path,
        r#"IFS= read -r init
printf '%s\n' '{"id":"adroit-init","result":{}}'
IFS= read -r initialized
IFS= read -r query
login=$(basename "$CODEX_HOME")
printf '{"id":"adroit-query","result":{"data":[{"id":"%s-model"}],"nextCursor":null}}\n' "$login"
"#,
    )
    .unwrap();
    path
}

#[tokio::test]
async fn two_codex_profiles_list_the_models_of_their_own_logins_and_a_late_answer_is_dropped() {
    use adroit_model::{Asked, ModelCatalog};
    let dir = tempfile::tempdir().unwrap();
    let mut s = settings(
        Agent::Codex,
        &model_listing(dir.path()),
        &dir.path().join("work"),
        None,
    );
    // The agent's own environment keeps whatever home adroit runs with, which
    // inside a Codex session is that session's; pinned here.
    s.profiles[0].launch.env.insert(
        "CODEX_HOME".into(),
        dir.path().join("own").display().to_string(),
    );
    let asked = |profile: &str, through: &str| Asked {
        profile: profile.parse().unwrap(),
        agent: Agent::Codex,
        home: (profile == "work").then(|| dir.path().join("work")),
        through: through.into(),
    };
    let mut catalog = ModelCatalog::default();
    catalog.configure(
        vec![asked("own", "1"), asked("work", "1")],
        "own".parse().unwrap(),
    );
    let generations = catalog.to_ask();

    let codex = adroit_host::programs::profiles(&s)
        .into_iter()
        .filter(|c| c.agent() == Agent::Codex)
        .map(|c| adroit_agents::AgentContext {
            adroit: ADROIT.into(),
            ..c
        });
    for context in codex {
        // A test binary and a shell started under a loaded machine may take
        // longer than a CLI is given to answer.
        let listed = adroit_agents::adapter(Agent::Codex)
            .models_within(context.clone(), dir.path().into(), Duration::from_secs(60))
            .await
            .unwrap();
        let (_, generation) = generations
            .iter()
            .find(|(a, _)| a.profile == context.profile)
            .unwrap();
        catalog.learn(*generation, listed);
    }

    let own: adroit_model::profile::ProfileId = "own".parse().unwrap();
    let work: adroit_model::profile::ProfileId = "work".parse().unwrap();
    assert_eq!(catalog.check(&own, Some("own-model")), Ok(()));
    assert!(catalog.check(&own, Some("work-model")).is_err());
    assert_eq!(catalog.check(&work, Some("work-model")), Ok(()));
    assert!(catalog.check(&work, Some("own-model")).is_err());

    // Asked again through another home, an answer to the earlier question
    // tells nothing of the profile now.
    catalog.configure(
        vec![asked("own", "1"), asked("work", "2")],
        "own".parse().unwrap(),
    );
    let (_, stale) = generations.iter().find(|(a, _)| a.profile == work).unwrap();
    catalog.learn(
        *stale,
        vec![adroit_model::ProviderModel {
            id: "stale-model".into(),
            resolved: None,
            name: "stale".into(),
            description: String::new(),
        }],
    );
    assert_eq!(
        catalog.check(&work, Some("anything")),
        Ok(()),
        "no list yet"
    );
    assert_eq!(catalog.check(&own, Some("own-model")), Ok(()));
    assert!(catalog.check(&own, Some("work-model")).is_err());
}

#[test]
fn two_pi_profiles_start_side_by_side_each_with_its_own_logins_sessions_and_skills() {
    let dir = tempfile::tempdir().unwrap();
    let home = dir.path().join("pi-work");
    let s = settings(Agent::Pi, &agent_program(dir.path()), &home, None);

    let own = started(&s, Agent::Pi, "own", &dir.path().join("own"));
    let work = started(&s, Agent::Pi, "work", &dir.path().join("work"));

    assert!(has(&own, "PI_CODING_AGENT_DIR=unset"), "{own:?}");
    assert!(has(&own, "OPENAI_API_KEY=inherited"), "{own:?}");
    assert!(has(&own, "AWS_PROFILE=inherited"), "{own:?}");
    assert!(
        !has(&own, "--no-skills") && !has(&own, "--session-dir"),
        "{own:?}"
    );
    assert!(
        has(&work, &format!("PI_CODING_AGENT_DIR={}", home.display())),
        "{work:?}"
    );
    for key in ["OPENAI_API_KEY", "ANTHROPIC_API_KEY", "AWS_PROFILE"] {
        assert!(has(&work, &format!("{key}=unset")), "{key} {work:?}");
    }
    assert!(has(&work, "--no-skills"), "{work:?}");
    let sessions = work.iter().position(|l| l == "--session-dir").unwrap();
    assert_eq!(
        work[sessions + 1],
        home.join("sessions").display().to_string()
    );
    // Both load adroit's extension and run Pi's own interface.
    for lines in [&own, &work] {
        assert!(has(lines, "-e") && has(lines, "regular"), "{lines:?}");
    }
}

/// A program standing in for Pi's RPC mode, which lists one model named after
/// the login it was started in: its state directory's last component.
fn pi_model_listing(dir: &Path) -> PathBuf {
    let path = dir.join("pi-rpc.sh");
    std::fs::write(
        &path,
        r#"IFS= read -r command
login=$(basename "${PI_CODING_AGENT_DIR:-/own}")
printf '{"type":"response","id":"adroit","command":"get_available_models","success":true,"data":{"models":[{"provider":"fake","id":"%s-model"}]}}\n' "$login"
"#,
    )
    .unwrap();
    path
}

#[tokio::test]
async fn two_pi_profiles_list_the_models_their_own_logins_can_run() {
    let dir = tempfile::tempdir().unwrap();
    let s = settings(
        Agent::Pi,
        &pi_model_listing(dir.path()),
        &dir.path().join("work"),
        None,
    );
    let mut listed = Vec::new();
    for context in adroit_host::programs::profiles(&s)
        .into_iter()
        .filter(|c| c.agent() == Agent::Pi)
    {
        let context = adroit_agents::AgentContext {
            adroit: ADROIT.into(),
            ..context
        };
        let models = adroit_agents::adapter(Agent::Pi)
            .models_within(context.clone(), dir.path().into(), Duration::from_secs(60))
            .await
            .unwrap();
        listed.push((
            context.profile.to_string(),
            models.into_iter().map(|m| m.id).collect::<Vec<_>>(),
        ));
    }
    assert_eq!(
        listed,
        [
            ("own".to_string(), vec!["fake/own-model".to_string()]),
            ("work".to_string(), vec!["fake/work-model".to_string()]),
        ]
    );
}

#[test]
fn two_opencode_profiles_start_side_by_side_each_with_only_its_account_s_folders_and_none_of_its_keys()
 {
    let dir = tempfile::tempdir().unwrap();
    let home = dir.path().join("opencode-work");
    let s = settings(Agent::OpenCode, &agent_program(dir.path()), &home, None);

    let own = started(&s, Agent::OpenCode, "own", &dir.path().join("own"));
    let work = started(&s, Agent::OpenCode, "work", &dir.path().join("work"));

    // The user's own environment is OpenCode's as adroit found it.
    for line in [
        "XDG_DATA_HOME=/outside/data",
        "GITHUB_TOKEN=inherited",
        "ANTHROPIC_API_KEY=inherited",
        "OPENCODE_DB=/inherited.db",
        "ADROIT_OUTSIDE_XDG_DATA_HOME=unset",
    ] {
        assert!(has(&own, line), "{line} {own:?}");
    }
    for line in [
        format!("XDG_DATA_HOME={}", home.join("data").display()),
        format!("XDG_CONFIG_HOME={}", home.join("config").display()),
        "GITHUB_TOKEN=unset".into(),
        "ANTHROPIC_API_KEY=unset".into(),
        "OPENAI_API_KEY=unset".into(),
        "AWS_PROFILE=unset".into(),
        "OPENCODE_DB=unset".into(),
        "ADROIT_OUTSIDE_XDG_DATA_HOME=/outside/data".into(),
    ] {
        assert!(has(&work, &line), "{line} {work:?}");
    }
    // Both run a server of their own on the conversation adroit chose.
    for lines in [&own, &work] {
        assert!(
            has(lines, "--standalone") && has(lines, "--session"),
            "{lines:?}"
        );
        assert!(has(lines, "--prompt=go"), "{lines:?}");
    }
}

#[test]
fn what_a_shell_s_startup_files_set_reaches_no_opencode_account_of_its_own() {
    let dir = tempfile::tempdir().unwrap();
    let shell = meddling_shell(dir.path());
    let home = dir.path().join("opencode-work");
    let s = settings(
        Agent::OpenCode,
        &agent_program(dir.path()),
        &home,
        Some(&shell),
    );

    let own = started(&s, Agent::OpenCode, "own", &dir.path().join("own"));
    let work = started(&s, Agent::OpenCode, "work", &dir.path().join("work"));

    assert!(has(&own, "OPENAI_API_KEY=from-rc"), "{own:?}");
    assert!(has(&work, "OPENAI_API_KEY=unset"), "{work:?}");
    assert!(has(&work, "OPENAI_EXTRA=unset"), "{work:?}");
    assert!(
        has(
            &work,
            &format!("XDG_DATA_HOME={}", home.join("data").display())
        ),
        "{work:?}"
    );
}

/// A program standing in for `opencode models`, which lists one model named
/// after the account it was started in: the folder its data root is in.
fn opencode_model_listing(dir: &Path) -> PathBuf {
    let path = dir.join("opencode-models.sh");
    std::fs::write(
        &path,
        r#"[ "$*" = 'models --standalone' ] || exit 1
login=$(basename "$(dirname "${XDG_DATA_HOME:-/own/data}")")
printf 'opencode/big-pickle\nfake/%s-model\n' "$login"
"#,
    )
    .unwrap();
    path
}

#[tokio::test]
async fn two_opencode_profiles_list_the_models_of_their_own_accounts() {
    let dir = tempfile::tempdir().unwrap();
    let mut s = settings(
        Agent::OpenCode,
        &opencode_model_listing(dir.path()),
        &dir.path().join("work"),
        None,
    );
    // The user's own environment keeps whatever data root adroit runs with;
    // pinned here.
    s.profiles[0]
        .launch
        .env
        .insert("XDG_DATA_HOME".into(), "/own/data".into());
    let mut listed = Vec::new();
    for context in adroit_host::programs::profiles(&s)
        .into_iter()
        .filter(|c| c.agent() == Agent::OpenCode)
    {
        let context = adroit_agents::AgentContext {
            adroit: ADROIT.into(),
            ..context
        };
        let models = adroit_agents::adapter(Agent::OpenCode)
            .models_within(context.clone(), dir.path().into(), Duration::from_secs(60))
            .await
            .unwrap();
        listed.push((
            context.profile.to_string(),
            models.into_iter().map(|m| m.id).collect::<Vec<_>>(),
        ));
    }
    assert_eq!(
        listed,
        [
            (
                "own".to_string(),
                vec!["opencode/big-pickle".to_string(), "fake/own-model".into()]
            ),
            (
                "work".to_string(),
                vec!["opencode/big-pickle".to_string(), "fake/work-model".into()]
            ),
        ]
    );
}
