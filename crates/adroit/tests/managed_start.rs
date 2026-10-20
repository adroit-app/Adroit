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
async fn human_starts_at_one_root_receive_independent_leaders_and_runs() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().join("control");
    let _service = Service::bind(&root).await.unwrap();
    let first = managed::prepare(
        &root,
        &Settings::default(),
        request(dir.path()),
        std::path::Path::new("/adroit"),
    )
    .await
    .unwrap();
    let second = managed::prepare(
        &root,
        &Settings::default(),
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
        &Settings::default(),
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
        &Settings::default(),
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
