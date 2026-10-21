mod support;
use adroit::control::{client::Client, managed, protocol::Command, service::Service};
use adroit::execution::{self, StartRequest};
use adroit::settings::Settings;
use adroit_model::*;
use adroit_term::{Daemon, Palette};
use std::sync::{
    Arc,
    atomic::{AtomicBool, AtomicUsize, Ordering},
};

#[tokio::test]
async fn an_authorized_independent_run_launches_a_new_leader_with_its_request() {
    use std::os::unix::fs::PermissionsExt;
    let dir = tempfile::Builder::new()
        .prefix("adroit-independent-")
        .tempdir_in("/tmp")
        .unwrap();
    std::fs::set_permissions(dir.path(), std::fs::Permissions::from_mode(0o700)).unwrap();
    let workspace = dir.path().join("workspace");
    std::fs::create_dir(&workspace).unwrap();
    let program = dir.path().join("provider");
    let receipt = dir.path().join("request-args");
    std::fs::write(
        &program,
        support::codex_script(
            r#"#!/bin/sh
printf '%s\n' "$@" > "$ADROIT_TEST_RECEIPT"
sleep 10
"#,
            None,
        ),
    )
    .unwrap();
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
    let human = Client::human(&root).unwrap();
    let leader: ManagedSession = serde_json::from_value(
        human
            .request(
                RequestId::new(),
                Command::RegisterLeader {
                    id: LogicalSessionId::new(),
                    agent: Agent::Codex,
                    cwd: workspace.clone(),
                    permissions: AgentPermissions::Default,
                },
            )
            .await
            .unwrap()["value"]
            .clone(),
    )
    .unwrap();
    let owner = leader.reference();
    let token: String = serde_json::from_value(
        human
            .request(RequestId::new(), Command::Grant { identity: owner })
            .await
            .unwrap(),
    )
    .unwrap();
    let agent = Client::session(&root, token);
    agent
        .request(
            RequestId::new(),
            Command::OpenRun {
                id: RunId::new(),
                brief: RunBrief {
                    title: "Original".into(),
                    goal: "Original work".into(),
                    ..Default::default()
                },
            },
        )
        .await
        .unwrap();
    let grant = human
        .request(
            RequestId::new(),
            Command::AuthorizeIndependentRun {
                leader: owner,
                workspace: workspace.clone(),
                brief: RunBrief {
                    title: "Independent".into(),
                    goal: "Inspect the separate request".into(),
                    ..Default::default()
                },
                agent: Agent::Codex,
                model: None,
                permissions: AgentPermissions::Default,
            },
        )
        .await
        .unwrap();
    let authorization_id: RequestId = serde_json::from_value(grant["value"]["id"].clone()).unwrap();
    let created = agent
        .request(
            RequestId::new(),
            Command::CreateIndependentRun { authorization_id },
        )
        .await
        .unwrap();
    let run_id: RunId = serde_json::from_value(created["value"]["run_id"].clone()).unwrap();
    let leader_id: LogicalSessionId =
        serde_json::from_value(created["value"]["leader_id"].clone()).unwrap();
    let mut settings = Settings::default();
    settings.codex.command = vec![program.to_string_lossy().into_owned()];
    settings.codex.env.insert(
        "ADROIT_TEST_RECEIPT".into(),
        receipt.to_string_lossy().into_owned(),
    );
    service
        .supervise_checked(
            daemon.clone(),
            env!("CARGO_BIN_EXE_adroit").into(),
            move || Ok(settings.clone()),
        )
        .unwrap();
    tokio::time::timeout(std::time::Duration::from_secs(8), async {
        loop {
            let snapshot = service
                .actor()
                .call(|store| store.snapshot())
                .await
                .unwrap();
            if snapshot.sessions.iter().any(|session| {
                session.binding.logical_id() == leader_id
                    && session.state == ManagedSessionState::Running
            }) && receipt.exists()
            {
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(20)).await;
        }
    })
    .await
    .unwrap();
    let text = std::fs::read_to_string(receipt).unwrap();
    assert!(text.contains("Inspect the separate request"), "{text}");
    let snapshot = service
        .actor()
        .call(|store| store.snapshot())
        .await
        .unwrap();
    assert_eq!(
        snapshot
            .runs
            .iter()
            .find(|run| run.id == run_id)
            .unwrap()
            .leader_id,
        leader_id
    );
    assert_eq!(
        snapshot
            .sessions
            .iter()
            .find(|session| session.binding.logical_id() == leader_id)
            .unwrap()
            .binding
            .role(),
        SessionRole::Leader
    );
    daemon_task.abort();
}

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
if [ "$ADROIT_LEADER_ID" != "$ADROIT_SESSION_ID" ]; then
    printf '{"op":"report_task","report":{"message_id":"%s","task":{"task_id":"%s","revision":1},"attempt_id":"%s","result":{"status":"succeeded","summary":"Classified the inputs","artifact":{"kind":"report","data":{"label":"ok"},"files":[],"reviewed_revision":null},"checks":[],"unresolved":[]}}}' "$ADROIT_TEST_REPORT_ID" "$ADROIT_TASK_ID" "$ADROIT_ATTEMPT_ID" | "$ADROIT_TEST_BINARY" control > report.json
else
    printf '{"op":"context"}' | "$ADROIT_TEST_BINARY" control > "leader-$ADROIT_SESSION_GENERATION-context.json"
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
    let invalid_settings = Arc::new(AtomicBool::new(true));
    let rejected_settings = Arc::new(AtomicUsize::new(0));
    service
        .supervise_checked(daemon.clone(), env!("CARGO_BIN_EXE_adroit").into(), {
            let invalid = invalid_settings.clone();
            let rejected = rejected_settings.clone();
            move || {
                if invalid.load(Ordering::SeqCst) {
                    rejected.fetch_add(1, Ordering::SeqCst);
                    Err("Invalid configuration".into())
                } else {
                    Ok(settings.clone())
                }
            }
        })
        .unwrap();
    tokio::time::timeout(std::time::Duration::from_secs(5), async {
        while rejected_settings.load(Ordering::SeqCst) < 2 {
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        }
    })
    .await
    .unwrap();
    let paused = service
        .actor()
        .call(|store| store.snapshot())
        .await
        .unwrap();
    assert!(paused.attempts.is_empty(), "{paused:?}");
    assert_eq!(paused.tasks[0].state, TaskState::Queued);
    invalid_settings.store(false, Ordering::SeqCst);
    let mut delivered = false;
    for _ in 0..400 {
        let snapshot = service
            .actor()
            .call(|store| store.snapshot())
            .await
            .unwrap();
        if snapshot.tasks[0].state == TaskState::Succeeded
            && snapshot.deliveries.iter().any(|delivery| {
                delivery.message_id == report_id && delivery.state == DeliveryState::Acknowledged
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
    tokio::time::timeout(std::time::Duration::from_secs(3), async {
        loop {
            let current = service
                .actor()
                .call(|store| store.snapshot())
                .await
                .unwrap();
            let worker = current
                .sessions
                .iter()
                .find(|session| session.binding.role() == SessionRole::Worker)
                .unwrap();
            if worker.state == ManagedSessionState::Exited {
                let native = daemon.sessions().await.unwrap();
                assert!(
                    native
                        .iter()
                        .all(|session| Some(format!("{:032x}", session.id)) != worker.terminal_id)
                );
                assert_eq!(current.reports.len(), 1);
                assert_eq!(current.tasks[0].state, TaskState::Succeeded);
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(20)).await;
        }
    })
    .await
    .expect("a reported Worker must release its native terminal");
    assert_eq!(snapshot.attempts.len(), 1);
    assert_eq!(snapshot.reports.len(), 1);
    assert_eq!(snapshot.runs[0].state, RunState::Active);
    assert!(
        std::fs::read_to_string(receipt)
            .unwrap()
            .contains(&report_id.to_string())
    );
    let old_leader = snapshot
        .sessions
        .iter()
        .find(|session| session.reference() == owner)
        .unwrap();
    let terminal = old_leader.terminal_id.clone().unwrap();
    daemon
        .end(u128::from_str_radix(&terminal, 16).unwrap())
        .await
        .unwrap();
    human
        .request(
            RequestId::new(),
            Command::ConfirmTerminalExit {
                identity: owner,
                terminal_id: terminal.clone(),
            },
        )
        .await
        .unwrap();
    invalid_settings.store(true, Ordering::SeqCst);
    rejected_settings.store(0, Ordering::SeqCst);
    human
        .request(RequestId::new(), Command::RestartLeader { identity: owner })
        .await
        .unwrap();
    tokio::time::timeout(std::time::Duration::from_secs(5), async {
        while rejected_settings.load(Ordering::SeqCst) < 2 {
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        }
    })
    .await
    .unwrap();
    let paused = service
        .actor()
        .call(|store| store.snapshot())
        .await
        .unwrap();
    let waiting_leader = paused
        .sessions
        .iter()
        .find(|session| session.reference().id == owner.id)
        .unwrap();
    assert_eq!(waiting_leader.generation, 2);
    assert_eq!(waiting_leader.state, ManagedSessionState::Preparing);
    assert!(waiting_leader.terminal_id.is_none());
    invalid_settings.store(false, Ordering::SeqCst);
    let snapshot = tokio::time::timeout(std::time::Duration::from_secs(5), async {
        loop {
            let current = service
                .actor()
                .call(|store| store.snapshot())
                .await
                .unwrap();
            if current.sessions.iter().any(|session| {
                session.reference().id == owner.id
                    && session.generation == 2
                    && session.state == ManagedSessionState::Running
            }) {
                break current;
            }
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        }
    })
    .await
    .unwrap();
    let new_leader = snapshot
        .sessions
        .iter()
        .find(|session| session.reference().id == owner.id)
        .unwrap();
    assert_ne!(new_leader.terminal_id.as_ref(), Some(&terminal));
    assert_eq!(new_leader.binding.run_id(), Some(run_id));
    let receipt = new_leader
        .cwd
        .as_ref()
        .unwrap()
        .join("leader-2-context.json");
    let context = tokio::time::timeout(
        adroit::control::protocol::TIMEOUT + std::time::Duration::from_secs(2),
        async {
            loop {
                if let Ok(bytes) = std::fs::read(&receipt)
                    && let Ok(value) = serde_json::from_slice::<serde_json::Value>(&bytes)
                {
                    break value;
                }
                tokio::time::sleep(std::time::Duration::from_millis(10)).await;
            }
        },
    )
    .await;
    let context = match context {
        Ok(context) => context,
        Err(error) => {
            let terminal =
                u128::from_str_radix(new_leader.terminal_id.as_ref().unwrap(), 16).unwrap();
            panic!(
                "{error}; receipt={receipt:?}; native output={:?}",
                daemon.visible(terminal).await
            );
        }
    };
    assert_eq!(context["result"]["Ok"]["session"]["generation"], 2);
    assert_eq!(context["result"]["Ok"]["run"]["id"], run_id.to_string());
    assert_eq!(snapshot.attempts.len(), 1);
    assert_eq!(snapshot.reports.len(), 1);
    assert_eq!(snapshot.runs[0].state, RunState::Active);
    for session in daemon.sessions().await.unwrap() {
        daemon.end(session.id).await.unwrap();
    }
    drop(service);
    daemon_task.abort();
}
