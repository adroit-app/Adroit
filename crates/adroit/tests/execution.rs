use adroit::execution::{ModelSource, StartRequest, resolve};
use adroit::settings::Settings;
use adroit_model::Agent;
use adroit_model::{AgentPermissions, CustomPermissions};
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
