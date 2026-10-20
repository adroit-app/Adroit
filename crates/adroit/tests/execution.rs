use adroit::execution::{ModelSource, StartRequest, resolve};
use adroit::settings::Settings;
use adroit_model::Agent;
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
