use adroit::control::{client::Client, protocol::Command, service::Service};
use adroit_model::{Agent, LogicalSessionId, RequestId, RunBrief, RunId};
use serde_json::json;

#[tokio::test]
async fn requests_survive_reconnection_and_daemon_restart_without_a_window() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().join("control");
    let service = Service::bind(&root).await.unwrap();
    let client = Client::human(&root).unwrap();
    let id = LogicalSessionId::new();
    let request = RequestId::new();
    let command = Command::RegisterLeader {
        permissions: adroit_model::AgentPermissions::Default,
        id,
        agent: Agent::Codex,
        cwd: dir.path().into(),
    };
    let first = client.request(request, command.clone()).await.unwrap();
    drop(service);
    let _service = Service::bind(&root).await.unwrap();
    let replay = Client::human(&root)
        .unwrap()
        .request(request, command)
        .await
        .unwrap();
    assert_eq!(first, replay);
    let events = client
        .request(
            RequestId::new(),
            Command::Events {
                after: 0,
                limit: 100,
            },
        )
        .await
        .unwrap();
    assert_eq!(events["events"].as_array().unwrap().len(), 1);
    let events = client
        .request(
            RequestId::new(),
            Command::Events {
                after: 1,
                limit: 100,
            },
        )
        .await
        .unwrap();
    assert_eq!(events, json!({"events": [], "next_cursor": 1}));
}

#[tokio::test]
async fn credentials_determine_the_actor_and_cannot_grant_another_identity() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().join("control");
    let _service = Service::bind(&root).await.unwrap();
    let human = Client::human(&root).unwrap();
    let mut actors = Vec::new();
    for _ in 0..2 {
        let id = LogicalSessionId::new();
        let reply = human
            .request(
                RequestId::new(),
                Command::RegisterLeader {
                    permissions: adroit_model::AgentPermissions::Default,
                    id,
                    agent: Agent::Claude,
                    cwd: dir.path().into(),
                },
            )
            .await
            .unwrap();
        let identity = serde_json::from_value(reply["value"].clone())
            .map(|s: adroit_model::ManagedSession| s.reference())
            .unwrap();
        let grant = human
            .request(RequestId::new(), Command::Grant { identity })
            .await
            .unwrap();
        actors.push(Client::session(&root, grant.as_str().unwrap().into()));
    }
    let run = RunId::new();
    actors[0]
        .request(
            RequestId::new(),
            Command::OpenRun {
                id: run,
                brief: RunBrief {
                    title: "Review".into(),
                    goal: "Review the change".into(),
                    ..Default::default()
                },
            },
        )
        .await
        .unwrap();
    assert!(
        actors[1]
            .request(
                RequestId::new(),
                Command::Grant {
                    identity: adroit_model::SessionRef {
                        id: LogicalSessionId::new(),
                        generation: 1
                    }
                }
            )
            .await
            .is_err()
    );
    assert!(
        actors[1]
            .request(RequestId::new(), Command::Snapshot)
            .await
            .is_err()
    );
    let snapshot = actors[0]
        .request(RequestId::new(), Command::Snapshot)
        .await
        .unwrap();
    assert_eq!(snapshot["runs"].as_array().unwrap().len(), 1);
    assert!(
        Client::session(&root, "incorrect".into())
            .request(RequestId::new(), Command::Snapshot)
            .await
            .is_err()
    );
}

#[tokio::test]
async fn slow_and_oversized_clients_do_not_hold_up_other_requests() {
    use tokio::io::{AsyncBufReadExt, AsyncWriteExt};
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().join("control");
    let _service = Service::bind(&root).await.unwrap();
    let _idle = tokio::net::UnixStream::connect(root.join("control.sock"))
        .await
        .unwrap();
    let mut large = tokio::net::UnixStream::connect(root.join("control.sock"))
        .await
        .unwrap();
    large.write_all(&vec![b'x'; 1_048_577]).await.unwrap();
    let mut line = String::new();
    tokio::io::BufReader::new(large)
        .read_line(&mut line)
        .await
        .unwrap();
    assert!(line.contains("invalid_request"));
    let snapshot = Client::human(&root)
        .unwrap()
        .request(RequestId::new(), Command::Snapshot)
        .await
        .unwrap();
    assert_eq!(snapshot["event_cursor"], 0);
    assert!(Service::bind(&root).await.is_err());
}

#[tokio::test]
async fn a_session_grant_survives_restart_and_cannot_select_another_actor() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().join("control");
    let service = Service::bind(&root).await.unwrap();
    let human = Client::human(&root).unwrap();
    let reply = human
        .request(
            RequestId::new(),
            Command::RegisterLeader {
                permissions: adroit_model::AgentPermissions::Default,
                id: LogicalSessionId::new(),
                agent: Agent::Codex,
                cwd: dir.path().into(),
            },
        )
        .await
        .unwrap();
    let session: adroit_model::ManagedSession =
        serde_json::from_value(reply["value"].clone()).unwrap();
    let grant = human
        .request(
            RequestId::new(),
            Command::Grant {
                identity: session.reference(),
            },
        )
        .await
        .unwrap();
    let agent = Client::session(&root, grant.as_str().unwrap().into());
    drop(service);
    let _service = Service::bind(&root).await.unwrap();
    agent
        .request(
            RequestId::new(),
            Command::OpenRun {
                id: RunId::new(),
                brief: RunBrief {
                    title: "Check".into(),
                    goal: "Check the code".into(),
                    ..Default::default()
                },
            },
        )
        .await
        .unwrap();
    assert!(
        agent
            .as_actor(session.reference())
            .request(RequestId::new(), Command::Snapshot)
            .await
            .is_err()
    );
}

#[test]
fn upgrading_an_existing_store_keeps_records_and_adds_session_credentials() {
    use adroit::control::store::Store;
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("control/state.sqlite3");
    let mut store = Store::open(&path).unwrap();
    let session = store
        .register_leader(
            RequestId::new(),
            LogicalSessionId::new(),
            Agent::Codex,
            dir.path().into(),
        )
        .unwrap()
        .value;
    drop(store);
    let connection = rusqlite::Connection::open(&path).unwrap();
    connection
        .execute_batch("DROP TABLE native_progress; DROP TABLE native_aliases; DROP TABLE completion_requests; DROP TABLE candidate_checks; DROP TABLE candidates; DROP TABLE coordination_replies; DROP TABLE coordination_notices; DROP TABLE run_completions; DROP TABLE session_launches; DROP TABLE task_reports; DROP TABLE run_work_areas; DROP TABLE work_areas; DROP TABLE session_credentials; PRAGMA user_version=1;")
        .unwrap();
    drop(connection);
    let mut store = Store::open(&path).unwrap();
    assert_eq!(store.snapshot().unwrap().sessions, vec![session.clone()]);
    let token = store.grant(session.reference()).unwrap();
    assert_eq!(store.authenticate(&token).unwrap(), session.reference());
}

#[tokio::test]
async fn the_installed_command_shape_reaches_a_daemon_without_opening_a_window() {
    use std::process::Stdio;
    use tokio::io::AsyncWriteExt;
    let dir = tempfile::Builder::new()
        .prefix("adroit-control-")
        .tempdir_in("/tmp")
        .unwrap();
    let root = dir.path().join("adroit/control");
    let mut daemon = tokio::process::Command::new(env!("CARGO_BIN_EXE_adroit"))
        .arg("daemon")
        .env("XDG_STATE_HOME", dir.path())
        .env("XDG_CONFIG_HOME", dir.path().join("config"))
        .env("ADROIT_INSTANCE", "default")
        .env("ADROIT_DAEMON", dir.path().join("felis/daemon.sock"))
        .stderr(Stdio::null())
        .kill_on_drop(true)
        .spawn()
        .unwrap();
    tokio::time::timeout(std::time::Duration::from_secs(5), async {
        while !root.join("control.sock").exists() {
            assert!(daemon.try_wait().unwrap().is_none());
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        }
    })
    .await
    .unwrap();
    let request = RequestId::new();
    let mut command = tokio::process::Command::new(env!("CARGO_BIN_EXE_adroit"))
        .args(["control", "--request-id", &request.to_string()])
        .env("XDG_STATE_HOME", dir.path())
        .env("ADROIT_INSTANCE", "default")
        .env_remove("ADROIT_CONTROL_TOKEN")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true)
        .spawn()
        .unwrap();
    command
        .stdin
        .take()
        .unwrap()
        .write_all(br#"{"op":"snapshot"}"#)
        .await
        .unwrap();
    let output = command.wait_with_output().await.unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let reply: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(reply["request_id"], request.to_string());
    assert_eq!(reply["result"]["Ok"]["event_cursor"], 0);
    daemon.kill().await.unwrap();
    daemon.wait().await.unwrap();
}
