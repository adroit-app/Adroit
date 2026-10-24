use adroit_host::delegation::launch::Delegation;
use adroit_host::execution::{ModelSource, StartRequest, resolve};
use adroit_host::settings::Settings;
use adroit_model::Agent;
use adroit_model::{AgentPermissions, CustomPermissions, SessionId};
use std::path::Path;

fn request(agent: Agent, model: Option<&str>) -> StartRequest {
    StartRequest {
        agent,
        model: model.map(String::from),
        name: "review".into(),
        cwd: "/repo".into(),
        prompt: "Review the changes".into(),
        ..Default::default()
    }
}

#[test]
fn automatic_agent_launches_record_the_current_time_and_preserve_explicit_start_times() {
    let now = || {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_millis() as u64
    };
    for agent in [Agent::Claude, Agent::Codex] {
        let before = now();
        let launch = resolve(
            &Settings::default(),
            request(agent, None),
            Path::new("/adroit"),
        )
        .unwrap();
        let started: u64 = adroit_term::tags::value(&launch.spawn.tags, "adroit.started")
            .unwrap()
            .parse()
            .unwrap();
        assert!(
            (before..=now()).contains(&started),
            "unexpected start time: {started}"
        );

        let mut explicit = request(agent, None);
        explicit.started_ms = 1234;
        let launch = resolve(&Settings::default(), explicit, Path::new("/adroit")).unwrap();
        assert_eq!(
            adroit_term::tags::value(&launch.spawn.tags, "adroit.started").as_deref(),
            Some("1234")
        );
    }
}

#[test]
fn a_task_model_overrides_only_the_target_session() {
    let mut settings = Settings::default();
    settings
        .profile_of_mut(Agent::Claude)
        .claude_mut()
        .unwrap()
        .model = "opus".into();
    let before = settings.clone();
    let first = resolve(
        &settings,
        request(Agent::Claude, Some("haiku")),
        Path::new("/adroit"),
    )
    .unwrap();
    let second = resolve(
        &settings,
        request(Agent::Claude, None),
        Path::new("/adroit"),
    )
    .unwrap();

    assert_eq!(first.model.as_deref(), Some("haiku"));
    assert_eq!(first.model_source, ModelSource::Explicit);
    assert_eq!(second.model.as_deref(), Some("opus"));
    assert_eq!(second.model_source, ModelSource::Settings);
    assert_eq!(settings, before);
}

#[test]
fn switching_agent_does_not_inherit_the_parent_model() {
    let mut settings = Settings::default();
    settings
        .profile_of_mut(Agent::Claude)
        .claude_mut()
        .unwrap()
        .model = "opus".into();
    let launch = resolve(&settings, request(Agent::Codex, None), Path::new("/adroit")).unwrap();

    assert_eq!(launch.agent, Agent::Codex);
    assert_eq!(launch.model, None);
    assert_eq!(launch.model_source, ModelSource::AgentDefault);
    assert!(!launch.spawn.args.iter().any(|a| a == "opus"));
}

#[test]
fn a_model_value_is_not_split_by_the_shell() {
    let mut settings = Settings::default();
    settings.profile_of_mut(Agent::Codex).launch.via_shell = true;
    let model = "custom model; $(touch never)";
    let launch = resolve(
        &settings,
        request(Agent::Codex, Some(model)),
        Path::new("/adroit"),
    )
    .unwrap();

    assert!(
        launch
            .spawn
            .args
            .windows(2)
            .any(|a| a == ["--model", model])
    );
    assert_eq!(launch.model.as_deref(), Some(model));
    assert_eq!(launch.spawn.cwd, Path::new("/repo"));
}

#[test]
fn an_empty_explicit_model_is_rejected_instead_of_using_a_default() {
    assert!(
        resolve(
            &Settings::default(),
            request(Agent::Claude, Some("  ")),
            Path::new("/adroit")
        )
        .is_err()
    );
}

#[test]
fn all_launch_arguments_and_environment_use_the_given_settings_snapshot() {
    let mut settings = Settings::default();
    settings.profile_of_mut(Agent::Codex).launch.command = vec!["/custom/codex".into()];
    settings
        .profile_of_mut(Agent::Codex)
        .launch
        .env
        .insert("CODEX_HOME".into(), "/custom/home".into());
    settings
        .profile_of_mut(Agent::Codex)
        .codex_mut()
        .unwrap()
        .model = "chosen-model".into();
    let launch = resolve(&settings, request(Agent::Codex, None), Path::new("/adroit")).unwrap();

    assert_eq!(launch.spawn.program, "/custom/codex");
    assert!(
        launch
            .spawn
            .env
            .contains(&("CODEX_HOME".into(), "/custom/home".into()))
    );
    assert!(
        launch
            .spawn
            .args
            .windows(2)
            .any(|a| a == ["--model", "chosen-model"])
    );
}

#[test]
fn a_read_only_session_keeps_its_permissions_when_its_model_changes() {
    let mut settings = Settings::default();
    settings
        .profile_of_mut(Agent::Codex)
        .codex_mut()
        .unwrap()
        .sandbox_mode = "danger-full-access".into();
    let mut request = request(Agent::Codex, Some("review-model"));
    request.permissions = AgentPermissions::ReadOnly;
    let launch = resolve(&settings, request, Path::new("/adroit")).unwrap();
    assert!(
        launch
            .spawn
            .args
            .windows(2)
            .any(|a| a == ["--sandbox", "read-only"])
    );
    assert!(!launch.spawn.args.iter().any(|a| a == "danger-full-access"));
    assert_eq!(launch.model.as_deref(), Some("review-model"));
}

#[test]
fn removed_reader_settings_are_rejected_instead_of_reinterpreted() {
    assert!(Settings::parse("[readers]\nmodel = 'haiku'\n").is_err());
    assert!(Settings::parse("[codex_readers]\nmodel = 'review-model'\n").is_err());
}

#[test]
fn a_read_only_claude_session_is_denied_editing_tools_and_writes_to_its_directory() {
    let mut settings = Settings::default();
    settings
        .profile_of_mut(Agent::Claude)
        .claude_mut()
        .unwrap()
        .permission_mode = "bypassPermissions".into();
    settings
        .profile_of_mut(Agent::Claude)
        .claude_mut()
        .unwrap()
        .disallowed_tools = vec!["WebFetch".into()];
    let mut request = request(Agent::Claude, Some("review-model"));
    request.permissions = AgentPermissions::ReadOnly;
    let launch = resolve(&settings, request, Path::new("/adroit")).unwrap();
    let flag = |name: &str| {
        launch
            .spawn
            .args
            .windows(2)
            .find(|pair| pair[0] == name)
            .map(|pair| pair[1].clone())
    };
    assert_eq!(
        flag("--disallowedTools").as_deref(),
        Some("WebFetch,Edit,Write,NotebookEdit")
    );
    let enforced: serde_json::Value =
        serde_json::from_str(&flag("--settings").expect("read-only settings")).unwrap();
    assert_eq!(enforced["sandbox"]["enabled"], true);
    assert_eq!(enforced["sandbox"]["failIfUnavailable"], true);
    assert_eq!(enforced["sandbox"]["allowUnsandboxedCommands"], false);
    assert_eq!(
        enforced["sandbox"]["filesystem"]["denyWrite"],
        serde_json::json!(["/repo"])
    );
    assert_eq!(launch.model.as_deref(), Some("review-model"));
}

#[test]
fn only_a_read_only_claude_session_carries_enforcement_settings() {
    let launch = resolve(
        &Settings::default(),
        request(Agent::Claude, None),
        Path::new("/adroit"),
    )
    .unwrap();
    assert!(!launch.spawn.args.iter().any(|arg| arg == "--settings"));
}

#[test]
fn custom_permissions_do_not_cross_providers_or_replace_the_model() {
    let permissions = AgentPermissions::Custom(Box::new(CustomPermissions::Codex {
        approval_policy: "on-request".into(),
        sandbox_mode: "workspace-write".into(),
    }));
    let mut codex = request(Agent::Codex, Some("chosen"));
    codex.permissions = permissions.clone();
    let launch = resolve(&Settings::default(), codex, Path::new("/adroit")).unwrap();
    assert_eq!(launch.model.as_deref(), Some("chosen"));
    assert!(
        launch
            .spawn
            .args
            .windows(2)
            .any(|a| a == ["--sandbox", "workspace-write"])
    );
    let mut claude = request(Agent::Claude, None);
    claude.permissions = permissions;
    assert!(resolve(&Settings::default(), claude, Path::new("/adroit")).is_err());
}

fn recorded(parent: Option<SessionId>) -> Delegation {
    Delegation::new(
        SessionId::new(),
        parent,
        "/private/delegation".into(),
        "test-secret-token".into(),
    )
}

#[test]
fn a_recorded_session_carries_its_credential_in_the_environment_alone() {
    for agent in Agent::ALL {
        let mut request = request(agent, None);
        request.delegation = Some(recorded(None));
        request
            .env
            .push(("ADROIT_DELEGATION_TOKEN".into(), "outer-token".into()));
        let mut settings = Settings::default();
        for launch in settings.profiles.iter_mut().map(|p| &mut p.launch) {
            launch
                .env
                .insert("ADROIT_DELEGATION_TOKEN".into(), "configured-token".into());
        }
        let launch = resolve(&settings, request, Path::new("/bin/adroit")).unwrap();
        let credentials: Vec<_> = launch
            .spawn
            .env
            .iter()
            .filter(|(name, _)| name == "ADROIT_DELEGATION_TOKEN")
            .collect();
        assert_eq!(
            credentials,
            [&("ADROIT_DELEGATION_TOKEN".into(), "test-secret-token".into())]
        );
        assert!(
            launch
                .spawn
                .args
                .iter()
                .chain(&launch.spawn.tags)
                .all(|arg| !arg.contains("test-secret-token"))
        );
    }
}

#[test]
fn claude_code_reaches_adroit_through_its_mcp_server_loaded_as_a_channel() {
    let mut request = request(Agent::Claude, None);
    request.delegation = Some(recorded(None));
    let launch = resolve(&Settings::default(), request, Path::new("/bin/adroit")).unwrap();
    let args = &launch.spawn.args;
    let config = args
        .windows(2)
        .find(|pair| pair[0] == "--mcp-config")
        .unwrap();
    let config: serde_json::Value = serde_json::from_str(&config[1]).unwrap();
    assert_eq!(
        config["mcpServers"]["adroit"]["args"],
        serde_json::json!(["mcp", "--channel"])
    );
    assert!(
        args.windows(2)
            .any(|pair| pair == ["--dangerously-load-development-channels", "server:adroit"])
    );
    assert!(
        args.windows(2)
            .any(|pair| pair == ["--allowedTools", "mcp__adroit__*"])
    );
    let told = args
        .windows(2)
        .find(|pair| pair[0] == "--append-system-prompt")
        .unwrap();
    assert!(
        told[1].contains("/bin/adroit terminal start"),
        "{}",
        told[1]
    );
}

#[test]
fn codex_reaches_adroit_through_its_mcp_server_and_keeps_its_own_instructions() {
    let mut request = request(Agent::Codex, None);
    request.delegation = Some(recorded(Some(SessionId::new())));
    request.developer_instructions = Some("Be terse.".into());
    let launch = resolve(&Settings::default(), request, Path::new("/bin/adroit")).unwrap();
    let args = &launch.spawn.args;
    assert!(
        args.iter()
            .any(|arg| arg == "mcp_servers.adroit.required=true")
    );
    let instructions = args
        .iter()
        .find(|arg| arg.starts_with("developer_instructions="))
        .unwrap();
    assert!(instructions.contains("Be terse."), "{instructions}");
    assert!(instructions.contains("report"), "{instructions}");
    assert!(
        instructions.contains("/bin/adroit terminal start"),
        "{instructions}"
    );
}

#[test]
fn a_launch_adroit_keeps_no_record_of_carries_no_mcp_server() {
    for agent in Agent::ALL {
        let launch = resolve(
            &Settings::default(),
            request(agent, None),
            Path::new("/bin/adroit"),
        )
        .unwrap();
        assert!(launch.spawn.args.iter().all(|arg| !arg.contains("mcp")));
        assert!(
            launch
                .spawn
                .env
                .iter()
                .all(|(name, _)| !name.starts_with("ADROIT_DELEGATION"))
        );
    }
}

#[test]
fn a_read_only_pi_session_is_given_only_the_tools_that_read() {
    let mut settings = Settings::default();
    settings.profile_of_mut(Agent::Pi).pi_mut().unwrap().tools =
        vec!["read".into(), "bash".into(), "edit".into()];
    let mut request = request(Agent::Pi, Some("fake/slow"));
    request.permissions = AgentPermissions::ReadOnly;
    let launch = resolve(&settings, request, Path::new("/adroit")).unwrap();
    assert!(
        launch
            .spawn
            .args
            .windows(2)
            .any(|a| a == ["--tools", "read,grep,find,ls"])
    );
    assert!(!launch.spawn.args.iter().any(|a| a.contains("bash")));
    assert_eq!(launch.model.as_deref(), Some("fake/slow"));
    assert!(
        launch
            .spawn
            .args
            .windows(2)
            .any(|a| a == ["--model", "fake/slow"])
    );
}

#[test]
fn custom_pi_permissions_are_the_tools_it_is_given() {
    let mut pi = request(Agent::Pi, None);
    pi.permissions = AgentPermissions::Custom(Box::new(CustomPermissions::Pi {
        tools: vec!["read".into(), "bash".into()],
    }));
    let launch = resolve(&Settings::default(), pi, Path::new("/adroit")).unwrap();
    assert!(
        launch
            .spawn
            .args
            .windows(2)
            .any(|a| a == ["--tools", "read,bash"])
    );
    let mut codex = request(Agent::Codex, None);
    codex.permissions = AgentPermissions::Custom(Box::new(CustomPermissions::Pi {
        tools: vec!["read".into()],
    }));
    assert!(resolve(&Settings::default(), codex, Path::new("/adroit")).is_err());
}

#[test]
fn a_pi_session_given_its_tools_keeps_adroit_s_when_it_carries_a_credential() {
    let tools = |permissions: AgentPermissions, delegated: bool| {
        let mut request = request(Agent::Pi, None);
        request.permissions = permissions;
        request.delegation = delegated.then(|| recorded(None));
        let launch = resolve(&Settings::default(), request, Path::new("/adroit")).unwrap();
        launch
            .spawn
            .args
            .windows(2)
            .find(|pair| pair[0] == "--tools")
            .map(|pair| pair[1].clone())
    };
    let custom = || {
        AgentPermissions::Custom(Box::new(CustomPermissions::Pi {
            tools: vec!["read".into(), "bash".into()],
        }))
    };
    assert_eq!(
        tools(AgentPermissions::ReadOnly, true).as_deref(),
        Some("read,grep,find,ls,mcp__adroit__*")
    );
    assert_eq!(
        tools(custom(), true).as_deref(),
        Some("read,bash,mcp__adroit__*")
    );
    assert_eq!(tools(custom(), false).as_deref(), Some("read,bash"));
    assert_eq!(tools(AgentPermissions::Default, true), None);

    let mut settings = Settings::default();
    settings.profile_of_mut(Agent::Pi).pi_mut().unwrap().tools = vec!["read".into()];
    let mut request = request(Agent::Pi, None);
    request.delegation = Some(recorded(None));
    let launch = resolve(&settings, request, Path::new("/adroit")).unwrap();
    assert!(
        launch
            .spawn
            .args
            .windows(2)
            .any(|pair| pair == ["--tools", "read,mcp__adroit__*"])
    );
}

#[test]
fn pi_reaches_adroit_through_the_extension_adroit_writes_for_it() {
    let mut request = request(Agent::Pi, None);
    request.delegation = Some(recorded(None));
    let launch = resolve(&Settings::default(), request, Path::new("/bin/adroit")).unwrap();
    let args = &launch.spawn.args;
    let extension = args.windows(2).find(|pair| pair[0] == "-e").unwrap();
    let script = std::fs::read_to_string(&extension[1]).unwrap();
    // The other tests here write the same file at once, naming adroits of
    // their own; which adroit it names is the adapter's tests' to pin.
    assert!(
        script.contains("const ADROIT = ") && script.contains("registerMcpServer"),
        "{script}"
    );
    assert!(args.iter().any(|arg| arg == "--skill"));
    let told = args
        .windows(2)
        .find(|pair| pair[0] == "--append-system-prompt")
        .unwrap();
    assert!(told[1].contains("start_session"), "{}", told[1]);
    assert!(
        launch
            .spawn
            .env
            .iter()
            .any(|(name, _)| name == "ADROIT_DELEGATION_TOKEN")
    );
    let prompt = args.iter().position(|arg| arg == "--").unwrap();
    assert_eq!(args[prompt + 1..], ["Review the changes"]);
    assert!(
        args.windows(2)
            .any(|pair| pair == ["--session-id", launch.conversation.as_str()])
    );
}
