use adroit::execution::{ModelSource, StartRequest, resolve};
use adroit::settings::Settings;
use adroit_model::Agent;
use adroit_model::{AgentPermissions, CustomPermissions, LogicalSessionId, RunId, SessionBinding};
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
fn a_read_only_worker_keeps_its_permissions_when_its_model_changes() {
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
fn read_only_claude_is_refused_until_enforcement_is_supported() {
    let mut request = request(Agent::Claude, None);
    request.permissions = AgentPermissions::ReadOnly;
    assert!(resolve(&Settings::default(), request, Path::new("/adroit")).is_err());
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

#[test]
fn a_worker_launch_keeps_its_explicit_assignment_even_at_the_repository_root() {
    use adroit_model::{AttemptId, LogicalSessionId, RunId, SessionBinding, TaskId};
    let binding = SessionBinding::worker(
        LogicalSessionId::new(),
        LogicalSessionId::new(),
        RunId::new(),
        TaskId::new(),
        AttemptId::new(),
    )
    .unwrap();
    let mut request = request(Agent::Codex, None);
    request.binding = Some(binding.clone());
    let launch = resolve(&Settings::default(), request, Path::new("/adroit")).unwrap();
    let raw = adroit_term::tags::value(&launch.spawn.tags, "adroit.binding").unwrap();
    assert_eq!(
        serde_json::from_str::<SessionBinding>(&raw).unwrap(),
        binding
    );
    assert_eq!(
        adroit_term::tags::value(&launch.spawn.tags, "adroit.binding-conversation").as_deref(),
        Some(launch.conversation.as_str())
    );
}

#[test]
fn a_codex_leader_requests_native_delegation_restrictions_per_launch() {
    use adroit_model::{LogicalSessionId, RunId, SessionBinding};
    let mut request = request(Agent::Codex, None);
    request.binding = Some(SessionBinding::leader(
        LogicalSessionId::new(),
        Some(RunId::new()),
    ));
    let launch = resolve(&Settings::default(), request, Path::new("/adroit")).unwrap();
    assert_eq!(launch.role_version, Some(1));
    for policy in [
        "agents.enabled=false",
        "features.multi_agent=false",
        "features.multi_agent_v2=false",
    ] {
        assert!(
            launch
                .spawn
                .args
                .windows(2)
                .any(|args| args == ["-c", policy])
        );
    }
    assert!(
        launch
            .spawn
            .args
            .iter()
            .any(|arg| arg.starts_with("developer_instructions=")
                && arg.contains("Leader")
                && arg.contains("internal subagents"))
    );
}

#[test]
fn a_claude_leader_combines_its_tool_restriction_with_custom_permissions() {
    use adroit_model::{LogicalSessionId, RunId, SessionBinding};
    let mut request = request(Agent::Claude, None);
    request.binding = Some(SessionBinding::leader(
        LogicalSessionId::new(),
        Some(RunId::new()),
    ));
    request.permissions = AgentPermissions::Custom(Box::new(CustomPermissions::Claude {
        permission_mode: "default".into(),
        disallowed_tools: vec!["Write".into()],
    }));
    let launch = resolve(&Settings::default(), request, Path::new("/adroit")).unwrap();
    let args = &launch.spawn.args;
    assert!(
        args.windows(2)
            .any(|args| args[0] == "--append-system-prompt" && args[1].contains("Leader"))
    );
    let flags: Vec<_> = args
        .windows(2)
        .filter(|args| args[0] == "--disallowedTools")
        .collect();
    assert_eq!(flags.len(), 1);
    for tool in ["Agent", "Task", "Write"] {
        assert!(flags[0][1].split(',').any(|name| name == tool));
    }
}

#[test]
fn worker_policy_does_not_disable_delegation_and_unmanaged_sessions_get_no_role() {
    use adroit_model::{AttemptId, LogicalSessionId, RunId, SessionBinding, TaskId};
    let mut worker = request(Agent::Codex, None);
    worker.binding = Some(
        SessionBinding::worker(
            LogicalSessionId::new(),
            LogicalSessionId::new(),
            RunId::new(),
            TaskId::new(),
            AttemptId::new(),
        )
        .unwrap(),
    );
    let launch = resolve(&Settings::default(), worker, Path::new("/adroit")).unwrap();
    assert_eq!(launch.role_version, Some(1));
    assert!(
        !launch
            .spawn
            .args
            .iter()
            .any(|arg| arg == "agents.enabled=false")
    );
    let unmanaged = resolve(
        &Settings::default(),
        request(Agent::Codex, None),
        Path::new("/adroit"),
    )
    .unwrap();
    assert_eq!(unmanaged.role_version, None);
    assert!(
        !unmanaged
            .spawn
            .args
            .iter()
            .any(|arg| arg.starts_with("developer_instructions="))
    );
}

#[test]
fn managed_control_uses_per_session_native_configuration_without_tokens_in_arguments() {
    use adroit::execution::ControlContext;
    use adroit_model::{LogicalSessionId, SessionBinding, SessionRef};
    for agent in [Agent::Claude, Agent::Codex] {
        let id = LogicalSessionId::new();
        let identity = SessionRef { id, generation: 3 };
        let mut request = request(agent, None);
        request.binding = Some(SessionBinding::leader(id, None));
        request.control = Some(ControlContext::new(
            identity,
            "/private/control".into(),
            "test-secret-token".into(),
        ));
        request
            .env
            .push(("ADROIT_CONTROL_TOKEN".into(), "outer-token".into()));
        let mut settings = Settings::default();
        settings
            .claude
            .env
            .insert("ADROIT_CONTROL_TOKEN".into(), "configured-token".into());
        settings
            .codex
            .env
            .insert("ADROIT_CONTROL_TOKEN".into(), "configured-token".into());
        let before = settings.clone();
        let launch = resolve(
            &settings,
            request,
            Path::new("/Applications/Adroit.app/Contents/MacOS/adroit"),
        )
        .unwrap();
        assert_eq!(settings, before);
        let credentials: Vec<_> = launch
            .spawn
            .env
            .iter()
            .filter(|(name, _)| name == "ADROIT_CONTROL_TOKEN")
            .collect();
        assert_eq!(credentials.len(), 1);
        assert_eq!(credentials[0].1, "test-secret-token");
        assert!(
            launch
                .spawn
                .env
                .contains(&("ADROIT_SESSION_GENERATION".into(), "3".into()))
        );
        assert!(
            launch
                .spawn
                .env
                .contains(&("ADROIT_CONTROL_ROOT".into(), "/private/control".into()))
        );
        assert!(
            launch
                .spawn
                .args
                .iter()
                .all(|arg| !arg.contains("test-secret-token"))
        );
        assert!(
            launch
                .spawn
                .tags
                .iter()
                .all(|tag| !tag.contains("test-secret-token"))
        );
        match agent {
            Agent::Claude => {
                assert!(
                    launch
                        .spawn
                        .args
                        .windows(2)
                        .any(|pair| pair == ["--allowedTools", "mcp__adroit_control__*"])
                );
                let config = launch
                    .spawn
                    .args
                    .windows(2)
                    .find(|pair| pair[0] == "--mcp-config")
                    .unwrap();
                let config: serde_json::Value = serde_json::from_str(&config[1]).unwrap();
                assert_eq!(
                    config["mcpServers"]["adroit_control"]["args"],
                    serde_json::json!(["control", "mcp", "--channel"])
                );
                assert!(launch.spawn.args.windows(2).any(|pair| pair
                    == [
                        "--dangerously-load-development-channels",
                        "server:adroit_control"
                    ]));
                assert!(
                    !launch
                        .spawn
                        .args
                        .iter()
                        .any(|arg| arg == "--strict-mcp-config")
                );
            }
            Agent::Codex => {
                assert!(launch.spawn.args.iter().any(
                    |arg| arg == "mcp_servers.adroit_control.tool_input_schema_max_bytes=65536"
                ));
                assert!(launch.spawn.args.iter().any(|arg| arg
                    == "mcp_servers.adroit_control.default_tools_approval_mode=\"approve\""));
                assert!(
                    launch
                        .spawn
                        .args
                        .iter()
                        .any(|arg| arg == "mcp_servers.adroit_control.required=true")
                );
            }
        }
    }
}

#[test]
fn a_control_context_must_match_the_explicit_session_assignment() {
    use adroit::execution::ControlContext;
    use adroit_model::{LogicalSessionId, SessionBinding, SessionRef};
    let mut request = request(Agent::Codex, None);
    request.binding = Some(SessionBinding::leader(LogicalSessionId::new(), None));
    request.control = Some(ControlContext::new(
        SessionRef {
            id: LogicalSessionId::new(),
            generation: 1,
        },
        "/private/control".into(),
        "test-secret-token".into(),
    ));
    let error = resolve(&Settings::default(), request, Path::new("/adroit")).unwrap_err();
    assert!(error.contains("assignment"));
    assert!(!error.contains("test-secret-token"));
}

#[test]
fn role_composition_refuses_oversized_user_instructions_without_truncating_them() {
    let base = "x".repeat(64 * 1024);
    let result = adroit::execution::resolve(
        &Settings::default(),
        StartRequest {
            agent: Agent::Codex,
            cwd: "/tmp".into(),
            binding: Some(SessionBinding::leader(
                LogicalSessionId::new(),
                Some(RunId::new()),
            )),
            developer_instructions: Some(base),
            ..Default::default()
        },
        std::path::Path::new("/adroit"),
    );
    assert!(
        result
            .unwrap_err()
            .contains("safe per-session launch limit")
    );
}
