use adroit::delegation::launch::Delegation;
use adroit::execution::{ModelSource, StartRequest, resolve};
use adroit::settings::Settings;
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
    settings.sessions.model = "opus".into();
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
    settings.sessions.model = "opus".into();
    let launch = resolve(&settings, request(Agent::Codex, None), Path::new("/adroit")).unwrap();

    assert_eq!(launch.agent, Agent::Codex);
    assert_eq!(launch.model, None);
    assert_eq!(launch.model_source, ModelSource::AgentDefault);
    assert!(!launch.spawn.args.iter().any(|a| a == "opus"));
}

#[test]
fn a_model_value_is_not_split_by_the_shell() {
    let mut settings = Settings::default();
    settings.codex.via_shell = true;
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
    settings.codex.command = vec!["/custom/codex".into()];
    settings
        .codex
        .env
        .insert("CODEX_HOME".into(), "/custom/home".into());
    settings.codex_sessions.model = "chosen-model".into();
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
    settings.codex_sessions.sandbox_mode = "danger-full-access".into();
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
    settings.sessions.permission_mode = "bypassPermissions".into();
    settings.sessions.disallowed_tools = vec!["WebFetch".into()];
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
    for agent in [Agent::Claude, Agent::Codex] {
        let mut request = request(agent, None);
        request.delegation = Some(recorded(None));
        request
            .env
            .push(("ADROIT_DELEGATION_TOKEN".into(), "outer-token".into()));
        let mut settings = Settings::default();
        for launch in [&mut settings.claude, &mut settings.codex] {
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
    assert!(args.iter().any(|arg| arg == "--append-system-prompt"));
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
}

#[test]
fn a_launch_adroit_keeps_no_record_of_carries_no_mcp_server() {
    for agent in [Agent::Claude, Agent::Codex] {
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
