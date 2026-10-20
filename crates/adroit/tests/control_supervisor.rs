mod support;
use adroit::control::{client::Client, managed, protocol::Command, service::Service};
use adroit::execution::{self, StartRequest};
use adroit::settings::Settings;
use adroit_model::*;
use adroit_term::{Daemon, Palette};

#[tokio::test]
async fn the_daemon_starts_workers_and_delivers_their_results_without_a_window() {
    use std::os::unix::fs::PermissionsExt;
    let dir = tempfile::Builder::new()
        .prefix("adroit-supervise-")
        .tempdir_in("/tmp")
        .unwrap();
    std::fs::set_permissions(dir.path(), std::fs::Permissions::from_mode(0o700)).unwrap();
    let source = dir.path().join("source");
    std::fs::create_dir(&source).unwrap();
    let program = dir.path().join("provider");
    std::fs::write(&program,support::codex_script(r#"#!/bin/sh
if [ "$1" = queue ]; then
    printf '%s\n' "$@" > "$ADROIT_TEST_RECEIPT"
    exit 0
fi
printf '\033]0;Codex · %s\007' "$ADROIT_SESSION_ID"
if [ -n "$ADROIT_TASK_ID" ]; then
    printf '{"op":"report_task","report":{"message_id":"%s","task":{"task_id":"%s","revision":1},"attempt_id":"%s","result":{"status":"succeeded","summary":"Classified the inputs","artifact":{"kind":"report","data":{"label":"ok"},"files":[],"reviewed_revision":null},"checks":[],"unresolved":[]}}}' "$ADROIT_TEST_REPORT_ID" "$ADROIT_TASK_ID" "$ADROIT_ATTEMPT_ID" | "$ADROIT_TEST_BINARY" control > report.json
fi
sleep 10
"#,None)).unwrap();
    std::fs::set_permissions(&program, std::fs::Permissions::from_mode(0o700)).unwrap();
    let root = dir.path().join("control");
    let mut service = Service::bind(&root).await.unwrap();
    let socket = dir.path().join("d.sock");
    let daemon_task = support::serve_process(socket.clone());
    let daemon = Daemon::new(socket);
    for _ in 0..100 {
        if daemon.sessions().await.is_ok() {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
    }
    let report_id = MessageId::new();
    let receipt = dir.path().join("notifications");
    let mut settings = Settings::default();
    settings.codex.command = vec![program.to_string_lossy().into_owned()];
    settings.codex.env.insert(
        "ADROIT_TEST_BINARY".into(),
        env!("CARGO_BIN_EXE_adroit").into(),
    );
    settings
        .codex
        .env
        .insert("ADROIT_TEST_REPORT_ID".into(), report_id.to_string());
    settings.codex.env.insert(
        "ADROIT_TEST_RECEIPT".into(),
        receipt.to_string_lossy().into_owned(),
    );
    let prepared = managed::prepare(
        &root,
        &settings,
        StartRequest {
            agent: Agent::Codex,
            name: "classification".into(),
            cwd: source,
            prompt: "Classify the inputs".into(),
            ..Default::default()
        },
        std::path::Path::new(env!("CARGO_BIN_EXE_adroit")),
    )
    .await
    .unwrap();
    let ownership = prepared.ownership.unwrap();
    let owner = ownership.identity();
    execution::spawn_headless_prepared(
        &daemon,
        prepared.resolved.spawn,
        &Palette::default(),
        move |terminal| async move { ownership.attach(terminal).await },
    )
    .await
    .unwrap();
    let human = Client::human(&root).unwrap();
    let snapshot: ControlSnapshot = serde_json::from_value(
        human
            .request(RequestId::new(), Command::Snapshot)
            .await
            .unwrap(),
    )
    .unwrap();
    let run_id = snapshot.runs[0].id;
    let task_id = TaskId::new();
    let leader = human.clone().as_actor(owner);
    leader
        .request(
            RequestId::new(),
            Command::PutTask {
                plan: TaskPlan {
                    id: task_id,
                    run_id,
                    expected_revision: None,
                    brief: TaskBrief {
                        instructions: "Classify the inputs".into(),
                        target: TargetPreference {
                            agent: Some(Agent::Codex),
                            ..Default::default()
                        },
                        ..Default::default()
                    },
                    dependencies: vec![],
                },
            },
        )
        .await
        .unwrap();
    leader
        .request(
            RequestId::new(),
            Command::QueueTask {
                version: TaskVersion {
                    task_id,
                    revision: 1,
                },
            },
        )
        .await
        .unwrap();
    service
        .supervise(
            daemon.clone(),
            env!("CARGO_BIN_EXE_adroit").into(),
            move || settings.clone(),
        )
        .unwrap();
    let mut delivered = false;
    for _ in 0..400 {
        let snapshot = service
            .actor()
            .call(|store| store.snapshot())
            .await
            .unwrap();
        if snapshot.tasks[0].state == TaskState::Succeeded
            && snapshot.deliveries.iter().any(|delivery| {
                delivery.message_id == report_id && delivery.state == DeliveryState::Accepted
            })
        {
            delivered = true;
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(20)).await;
    }
    let snapshot = service
        .actor()
        .call(|store| store.snapshot())
        .await
        .unwrap();
    assert!(delivered, "{snapshot:?}");
    assert_eq!(snapshot.attempts.len(), 1);
    assert_eq!(snapshot.reports.len(), 1);
    assert_eq!(snapshot.runs[0].state, RunState::Active);
    assert!(
        std::fs::read_to_string(receipt)
            .unwrap()
            .contains(&report_id.to_string())
    );
    for session in snapshot.sessions {
        if let Some(id) = session.terminal_id {
            daemon
                .end(u128::from_str_radix(&id, 16).unwrap())
                .await
                .unwrap();
        }
    }
    drop(service);
    daemon_task.abort();
}
