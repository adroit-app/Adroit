use adroit::control::{client::Client, protocol::Command, service::Service};
use adroit_model::*;
use std::path::{Path, PathBuf};

fn assigned(
    store: &mut adroit::control::store::Store,
    path: &Path,
) -> (SessionRef, SessionRef, RunId, TaskVersion) {
    let path = path.canonicalize().unwrap();
    let leader = store
        .register_leader(
            RequestId::new(),
            LogicalSessionId::new(),
            Agent::Codex,
            path.clone(),
        )
        .unwrap()
        .value
        .reference();
    let run = RunId::new();
    store
        .open_run(
            RequestId::new(),
            leader,
            run,
            RunBrief {
                title: "Output".into(),
                goal: "Produce the requested output".into(),
                ..Default::default()
            },
        )
        .unwrap();
    let version = TaskVersion {
        task_id: TaskId::new(),
        revision: 1,
    };
    store
        .put_task(
            RequestId::new(),
            leader,
            TaskPlan {
                id: version.task_id,
                run_id: run,
                expected_revision: None,
                brief: TaskBrief {
                    instructions: "Produce the output".into(),
                    ..Default::default()
                },
                dependencies: vec![],
            },
        )
        .unwrap();
    store.queue_task(RequestId::new(), leader, version).unwrap();
    let attempt = store
        .reserve_attempt(
            RequestId::new(),
            leader,
            version,
            TargetSelection::default(),
        )
        .unwrap()
        .value;
    let area = store
        .register_work_area(
            RequestId::new(),
            leader,
            run,
            WorkArea {
                id: WorkAreaId::new(),
                backend: WorkAreaBackend::Directory,
                path: path.clone(),
                source: path.clone(),
                source_root: path.clone(),
                repository: None,
                base_revision: None,
                native_name: None,
                description: "Assigned".into(),
                state: WorkAreaState::Ready,
                error: None,
            },
            false,
        )
        .unwrap()
        .value;
    store
        .bind_work_area(RequestId::new(), leader, version, attempt.id, area.id)
        .unwrap();
    store
        .begin_start(RequestId::new(), leader, version, attempt.id)
        .unwrap();
    let worker = SessionRef {
        id: attempt.session_id,
        generation: 1,
    };
    let terminal = attempt.id.to_string().replace("-", "");
    store
        .attach_session(
            RequestId::new(),
            worker,
            path,
            terminal,
            MessageId::new().to_string(),
        )
        .unwrap();
    (leader, worker, run, version)
}

async fn fixture(
    path: PathBuf,
    root: &Path,
    service: &Service,
) -> (Client, Client, SessionRef, TaskVersion, RunId) {
    let (_leader, worker, run, version, leader_token, worker_token) = service
        .actor()
        .call(move |store| {
            let (leader, worker, run, version) = assigned(store, &path);
            Ok((
                leader,
                worker,
                run,
                version,
                store.grant(leader)?,
                store.grant(worker)?,
            ))
        })
        .await
        .unwrap();
    (
        Client::session(root, leader_token),
        Client::session(root, worker_token),
        worker,
        version,
        run,
    )
}

#[tokio::test]
async fn file_evidence_uses_the_credential_assignment_and_rejects_leaders_and_stale_work() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("output.txt"), "Result").unwrap();
    let root = dir.path().join("control");
    let service = Service::bind(&root).await.unwrap();
    let (leader, worker, identity, version, run) =
        fixture(dir.path().into(), &root, &service).await;
    let context: AgentContext = serde_json::from_value(
        worker
            .request(RequestId::new(), Command::Context)
            .await
            .unwrap(),
    )
    .unwrap();
    assert_eq!(context.session.reference(), identity);
    assert_eq!(
        context.work_area.unwrap().path,
        dir.path().canonicalize().unwrap()
    );
    let fingerprint = adroit::control::files::capture(&worker, vec!["output.txt".into()])
        .await
        .unwrap();
    assert_eq!(
        fingerprint,
        adroit::candidates::capture_files(
            &dir.path().canonicalize().unwrap(),
            &["output.txt".into()]
        )
        .unwrap()
        .fingerprint
    );
    assert!(
        adroit::control::files::capture(&leader, vec!["output.txt".into()])
            .await
            .is_err()
    );
    leader
        .request(
            RequestId::new(),
            Command::PutTask {
                plan: TaskPlan {
                    id: version.task_id,
                    run_id: run,
                    expected_revision: Some(1),
                    brief: TaskBrief {
                        instructions: "Revised output".into(),
                        ..Default::default()
                    },
                    dependencies: vec![],
                },
            },
        )
        .await
        .unwrap();
    assert!(
        adroit::control::files::capture(&worker, vec!["output.txt".into()])
            .await
            .is_err()
    );
}

#[tokio::test]
async fn the_cli_ignores_a_forged_session_id_and_returns_the_assigned_output_fingerprint() {
    use std::process::Stdio;
    use tokio::io::AsyncWriteExt;
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("output.txt"), "Result").unwrap();
    let root = dir.path().join("control");
    let service = Service::bind(&root).await.unwrap();
    let (_, _, identity, _, _) = fixture(dir.path().into(), &root, &service).await;
    let token = service
        .actor()
        .call(move |store| store.grant(identity))
        .await
        .unwrap();
    let mut process = tokio::process::Command::new(env!("CARGO_BIN_EXE_adroit"));
    process
        .args(["control", "fingerprint"])
        .env("ADROIT_CONTROL_ROOT", &root)
        .env("ADROIT_CONTROL_TOKEN", token)
        .env("ADROIT_SESSION_ID", LogicalSessionId::new().to_string())
        .env("XDG_CONFIG_HOME", dir.path().join("config"))
        .env("XDG_STATE_HOME", dir.path().join("state"))
        .env("XDG_DATA_HOME", dir.path().join("data"))
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let mut child = process.spawn().unwrap();
    child
        .stdin
        .take()
        .unwrap()
        .write_all(b"[\"output.txt\"]")
        .await
        .unwrap();
    let output = child.wait_with_output().await.unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let value: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(
        value["result"]["Ok"],
        adroit::candidates::capture_files(
            &dir.path().canonicalize().unwrap(),
            &["output.txt".into()]
        )
        .unwrap()
        .fingerprint
    );
}

#[tokio::test]
async fn the_native_mcp_tool_can_capture_a_workers_fingerprint_without_shell_access() {
    use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("output.txt"), "Result").unwrap();
    let root = dir.path().join("control");
    let service = Service::bind(&root).await.unwrap();
    let (_, client, _, _, _) = fixture(dir.path().into(), &root, &service).await;
    let (input, mut send) = tokio::io::duplex(8192);
    let (output, receive) = tokio::io::duplex(8192);
    let bridge = tokio::spawn(adroit::control::bridge::run(
        BufReader::new(input),
        output,
        client,
        false,
    ));
    let mut receive = BufReader::new(receive);
    send.write_all(b"{\"jsonrpc\":\"2.0\",\"id\":1,\"method\":\"initialize\",\"params\":{\"protocolVersion\":\"2025-06-18\"}}\n").await.unwrap();
    let mut line = String::new();
    receive.read_line(&mut line).await.unwrap();
    send.write_all(b"{\"jsonrpc\":\"2.0\",\"id\":2,\"method\":\"tools/call\",\"params\":{\"name\":\"fingerprint_files\",\"arguments\":{\"paths\":[\"output.txt\"]}}}\n").await.unwrap();
    line.clear();
    receive.read_line(&mut line).await.unwrap();
    let reply: serde_json::Value = serde_json::from_str(&line).unwrap();
    assert_eq!(reply["result"]["isError"], false, "{reply}");
    let result: serde_json::Value =
        serde_json::from_str(reply["result"]["content"][0]["text"].as_str().unwrap()).unwrap();
    assert_eq!(
        result["Ok"],
        adroit::candidates::capture_files(
            &dir.path().canonicalize().unwrap(),
            &["output.txt".into()]
        )
        .unwrap()
        .fingerprint
    );
    drop(send);
    bridge.await.unwrap().unwrap();
}
