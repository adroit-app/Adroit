mod support;
use adroit::control::{client::Client, managed, protocol::Command, service::Service};
use adroit::execution::StartRequest;
use adroit::settings::Settings;
use adroit_model::*;

fn request(cwd: &std::path::Path) -> StartRequest {
    StartRequest {
        agent: Agent::Codex,
        name: "classification".into(),
        cwd: cwd.into(),
        prompt: "Classify the available inputs".into(),
        permissions: AgentPermissions::ReadOnly,
        ..Default::default()
    }
}

#[tokio::test]
async fn standalone_starts_at_a_repository_root_need_no_controller_or_role_policy() {
    use adroit::execution::StartMode;
    let dir = tempfile::tempdir().unwrap();
    std::fs::create_dir(dir.path().join(".git")).unwrap();
    for agent in Agent::ALL {
        let mut requested = request(dir.path());
        requested.agent = agent;
        requested.permissions = AgentPermissions::Default;
        requested.mode = StartMode::Standalone;
        let prepared = managed::prepare(
            &dir.path().join("no-controller"),
            &Settings::default(),
            requested,
            std::path::Path::new("/adroit"),
        )
        .await
        .unwrap();
        assert!(prepared.ownership.is_none());
        assert_eq!(prepared.resolved.role_version, None);
        assert!(
            !prepared
                .resolved
                .spawn
                .args
                .iter()
                .any(|arg| { arg.contains("adroit_control") || arg == "agents.enabled=false" })
        );
        assert!(
            !prepared
                .resolved
                .spawn
                .env
                .iter()
                .any(|(key, _)| adroit::env::reserved_control(key))
        );
        assert!(
            !prepared
                .resolved
                .spawn
                .tags
                .iter()
                .any(|tag| tag.starts_with("adroit.binding"))
        );
        assert!(!dir.path().join("no-controller").exists());
    }
}

#[tokio::test]
async fn an_explicit_leader_can_start_in_a_nested_directory() {
    use adroit::execution::StartMode;
    let dir = tempfile::tempdir().unwrap();
    let output = std::process::Command::new("jj")
        .args(["git", "init", "--colocate"])
        .current_dir(dir.path())
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let nested = dir.path().join("nested");
    std::fs::create_dir(&nested).unwrap();
    let root = dir.path().join("control");
    let _service = Service::bind(&root).await.unwrap();
    let mut requested = request(&nested);
    requested.mode = StartMode::Leader;
    let prepared = managed::prepare(
        &root,
        &native_settings(),
        requested,
        std::path::Path::new("/adroit"),
    )
    .await
    .unwrap();
    assert!(prepared.ownership.is_some());
    assert!(prepared.resolved.role_version.is_some());
}

#[tokio::test]
async fn human_starts_at_one_root_receive_independent_leaders_and_runs() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().join("control");
    let _service = Service::bind(&root).await.unwrap();
    let first = managed::prepare(
        &root,
        &native_settings(),
        request(dir.path()),
        std::path::Path::new("/adroit"),
    )
    .await
    .unwrap();
    let second = managed::prepare(
        &root,
        &native_settings(),
        request(dir.path()),
        std::path::Path::new("/adroit"),
    )
    .await
    .unwrap();
    assert_ne!(
        first.ownership.as_ref().unwrap().identity(),
        second.ownership.as_ref().unwrap().identity()
    );
    let snapshot = Client::human(&root)
        .unwrap()
        .request(RequestId::new(), Command::Snapshot)
        .await
        .unwrap();
    let snapshot: ControlSnapshot = serde_json::from_value(snapshot).unwrap();
    assert_eq!(snapshot.sessions.len(), 2);
    assert_eq!(snapshot.runs.len(), 2);
    assert_eq!(snapshot.work_areas.len(), 1);
    assert_eq!(snapshot.run_work_areas.len(), 2);
    assert!(
        snapshot
            .run_work_areas
            .iter()
            .all(|association| association.is_default)
    );
    assert_eq!(
        snapshot.work_areas[0].path,
        dir.path().canonicalize().unwrap()
    );
    assert!(
        snapshot
            .sessions
            .iter()
            .all(|session| session.binding.role() == SessionRole::Leader
                && session.permissions == AgentPermissions::ReadOnly)
    );
    assert_ne!(snapshot.runs[0].leader_id, snapshot.runs[1].leader_id);
    assert!(
        first
            .resolved
            .spawn
            .args
            .iter()
            .any(|arg| arg == "agents.enabled=false")
    );
}

#[tokio::test]
async fn a_nested_repository_directory_does_not_infer_a_new_leader() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::create_dir(dir.path().join(".git")).unwrap();
    let nested = dir.path().join("nested");
    std::fs::create_dir(&nested).unwrap();
    let prepared = managed::prepare(
        &dir.path().join("no-service"),
        &native_settings(),
        request(&nested),
        std::path::Path::new("/adroit"),
    )
    .await
    .unwrap();
    assert!(prepared.ownership.is_none());
    assert_eq!(prepared.resolved.role_version, None);
    assert!(
        !prepared
            .resolved
            .spawn
            .args
            .iter()
            .any(|arg| arg.contains("mcp_servers.adroit_control"))
    );
}

#[tokio::test]
async fn a_failed_human_launch_is_recorded_without_a_dangling_active_run() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().join("control");
    let _service = Service::bind(&root).await.unwrap();
    let prepared = managed::prepare(
        &root,
        &native_settings(),
        request(dir.path()),
        std::path::Path::new("/adroit"),
    )
    .await
    .unwrap();
    prepared
        .ownership
        .unwrap()
        .failed(&adroit_term::Error::NotFound("missing-provider".into()))
        .await;
    let snapshot = Client::human(&root)
        .unwrap()
        .request(RequestId::new(), Command::Snapshot)
        .await
        .unwrap();
    let snapshot: ControlSnapshot = serde_json::from_value(snapshot).unwrap();
    assert_eq!(snapshot.sessions[0].state, ManagedSessionState::Failed);
    assert_eq!(snapshot.runs[0].state, RunState::Failed);
}

#[test]
fn a_managed_agent_cannot_create_an_unassigned_peer_through_session_start() {
    let output = std::process::Command::new(env!("CARGO_BIN_EXE_adroit"))
        .args(["session", "start", "--", "Implement a task"])
        .env("ADROIT_CONTROL_TOKEN", "scoped-session")
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(2));
    assert!(String::from_utf8(output.stderr).unwrap().contains("task"));
}

#[tokio::test]
async fn independent_leaders_at_one_native_workspace_share_its_default_identity() {
    let dir = tempfile::tempdir().unwrap();
    let source = dir.path().join("source");
    std::fs::create_dir(&source).unwrap();
    for args in [
        vec!["git", "init", "--colocate"],
        vec!["describe", "-m", "test: establish a shared default"],
    ] {
        let output = std::process::Command::new("jj")
            .args([
                "--config",
                "user.name=Test",
                "--config",
                "user.email=test@example.invalid",
            ])
            .args(args)
            .current_dir(&source)
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
    let root = dir.path().join("control");
    let _service = Service::bind(&root).await.unwrap();
    for _ in 0..2 {
        managed::prepare(
            &root,
            &native_settings(),
            request(&source),
            std::path::Path::new("/adroit"),
        )
        .await
        .unwrap();
    }
    let snapshot: ControlSnapshot = serde_json::from_value(
        Client::human(&root)
            .unwrap()
            .request(RequestId::new(), Command::Snapshot)
            .await
            .unwrap(),
    )
    .unwrap();
    assert_eq!(snapshot.runs.len(), 2);
    assert_eq!(snapshot.work_areas.len(), 1);
    assert_eq!(snapshot.work_areas[0].backend, WorkAreaBackend::Jj);
    assert_eq!(snapshot.work_areas[0].path, source.canonicalize().unwrap());
    assert_eq!(snapshot.run_work_areas.len(), 2);
    assert!(
        snapshot
            .run_work_areas
            .iter()
            .all(|association| association.is_default
                && association.work_area_id == snapshot.work_areas[0].id)
    );
}

#[tokio::test]
async fn a_lost_ownership_reply_before_release_is_a_confirmed_abort_after_the_gate_is_stopped() {
    use adroit::execution;
    use adroit_model::{ManagedSessionState, RunState};
    use adroit_term::{Daemon, Palette};
    use std::os::unix::fs::PermissionsExt;
    let dir = tempfile::tempdir().unwrap();
    std::fs::set_permissions(dir.path(), std::fs::Permissions::from_mode(0o700)).unwrap();
    let source = dir.path().join("source");
    std::fs::create_dir(&source).unwrap();
    let marker = source.join("must-not-run");
    let root = dir.path().join("control");
    let service = Service::bind(&root).await.unwrap();
    let socket = dir.path().join("d.sock");
    let native = support::serve_process(socket.clone());
    let daemon = Daemon::new(socket);
    tokio::time::timeout(std::time::Duration::from_secs(3), async {
        while daemon.sessions().await.is_err() {
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        }
    })
    .await
    .unwrap();
    let mut settings = native_settings();
    settings.codex.command = vec![
        "/bin/sh".into(),
        "-c".into(),
        support::codex_script("touch \"$ADROIT_START_ABORT_MARKER\"; sleep 5", None),
        "native-provider".into(),
    ];
    settings.codex.env.insert(
        "ADROIT_START_ABORT_MARKER".into(),
        marker.to_string_lossy().into_owned(),
    );
    let prepared = managed::prepare(
        &root,
        &settings,
        request(&source),
        std::path::Path::new("/adroit"),
    )
    .await
    .unwrap();
    let ownership = prepared.ownership.unwrap();
    let callback = ownership.clone();
    let result = execution::spawn_headless_prepared(
        &daemon,
        prepared.resolved.spawn,
        &Palette::default(),
        move |terminal| async move {
            callback.attach(terminal).await?;
            Err(adroit_term::Error::Protocol(
                "The ownership reply was lost".into(),
            ))
        },
    )
    .await;
    let error = result.unwrap_err();
    assert!(matches!(
        &error,
        adroit_term::Error::PreparationAborted { .. }
    ));
    ownership.failed(&error).await;
    let snapshot = service
        .actor()
        .call(|store| store.snapshot())
        .await
        .unwrap();
    assert_eq!(snapshot.sessions[0].state, ManagedSessionState::Failed);
    assert_eq!(snapshot.runs[0].state, RunState::Failed);
    assert!(!marker.exists());
    assert!(daemon.sessions().await.unwrap().is_empty());
    native.abort();
}

#[tokio::test]
async fn private_launch_snapshots_preserve_requested_and_settings_selected_models_separately() {
    for explicit in [false, true] {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("control");
        let service = Service::bind(&root).await.unwrap();
        let mut settings = native_settings();
        settings.codex_sessions.model = "settings-model".into();
        let mut requested = request(dir.path());
        requested.model = explicit.then(|| "explicit-model".into());
        let prepared =
            managed::prepare(&root, &settings, requested, std::path::Path::new("/adroit"))
                .await
                .unwrap();
        let identity = prepared.ownership.unwrap().identity();
        let runtime = service
            .actor()
            .call(move |store| store.runtime(identity))
            .await
            .unwrap()
            .unwrap();
        let target = runtime.target.unwrap();
        assert_eq!(target.agent, Agent::Codex);
        assert_eq!(
            target.requested_model,
            explicit.then(|| "explicit-model".into())
        );
        assert_eq!(
            target.model,
            Some(
                if explicit {
                    "explicit-model"
                } else {
                    "settings-model"
                }
                .into()
            )
        );
        assert_eq!(
            target.model_source,
            if explicit {
                ModelSource::Explicit
            } else {
                ModelSource::Settings
            }
        );
        assert_eq!(target.permissions, AgentPermissions::ReadOnly);
        assert_eq!(prepared.resolved.requested_model, target.requested_model);
    }
}

#[tokio::test]
async fn managed_roles_append_to_effective_codex_developer_instructions_without_replacing_them() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().join("control");
    let service = Service::bind(&root).await.unwrap();
    let base = "Keep the user's developer policy.\nPreserve literal 'quoted' content.";
    let mut settings = native_settings();
    settings.codex.command = vec![
        "/bin/sh".into(),
        "-c".into(),
        support::codex_script("sleep 10", Some(base)),
        "native-provider".into(),
    ];
    let prepared = managed::prepare(
        &root,
        &settings,
        request(dir.path()),
        std::path::Path::new("/adroit"),
    )
    .await
    .unwrap();
    let encoded = prepared
        .resolved
        .spawn
        .args
        .iter()
        .find_map(|arg| arg.strip_prefix("developer_instructions="))
        .unwrap();
    let instructions: String = serde_json::from_str(encoded).unwrap();
    assert!(instructions.starts_with(base));
    assert!(instructions.contains("You are the Leader of one Adroit request"));
    let identity = prepared.ownership.unwrap().identity();
    let runtime = service
        .actor()
        .call(move |store| store.runtime(identity))
        .await
        .unwrap()
        .unwrap();
    assert_eq!(runtime.developer_instructions.as_deref(), Some(base));
}

#[tokio::test]
async fn unavailable_effective_codex_configuration_refuses_the_managed_launch_without_losing_user_policy()
 {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().join("control");
    let service = Service::bind(&root).await.unwrap();
    let mut settings = native_settings();
    settings.codex.command = vec!["/usr/bin/false".into()];
    assert!(
        managed::prepare(
            &root,
            &settings,
            request(dir.path()),
            std::path::Path::new("/adroit")
        )
        .await
        .is_err()
    );
    let snapshot = service
        .actor()
        .call(|store| store.snapshot())
        .await
        .unwrap();
    assert_eq!(snapshot.sessions[0].state, ManagedSessionState::Failed);
    assert_eq!(snapshot.runs[0].state, RunState::Failed);
}

fn native_settings() -> Settings {
    let mut settings = Settings::default();
    settings.codex.command = vec![
        "/bin/sh".into(),
        "-c".into(),
        support::codex_script("sleep 10", None),
        "native-provider".into(),
    ];
    settings
}
