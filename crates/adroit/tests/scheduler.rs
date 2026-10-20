mod support;
use adroit::control::{actor::Actor, scheduler::Scheduler};
use adroit::settings::Settings;
use adroit_model::*;
use adroit_term::Daemon;

async fn planned(actor: &Actor, source: &std::path::Path, count: usize) -> (SessionRef, RunId) {
    let source = source.to_path_buf();
    actor
        .call(move |store| {
            let session = store
                .register_leader(
                    RequestId::new(),
                    LogicalSessionId::new(),
                    Agent::Codex,
                    source,
                )?
                .value;
            let identity = session.reference();
            let run_id = RunId::new();
            store.open_run(
                RequestId::new(),
                identity,
                run_id,
                RunBrief {
                    title: "Classification".into(),
                    goal: "Classify the inputs".into(),
                    ..Default::default()
                },
            )?;
            for _ in 0..count {
                let id = TaskId::new();
                store.put_task(
                    RequestId::new(),
                    identity,
                    TaskPlan {
                        id,
                        run_id,
                        expected_revision: None,
                        brief: TaskBrief {
                            instructions: "Classify the inputs".into(),
                            target: TargetPreference {
                                agent: Some(Agent::Codex),
                                model: Some("test-small".into()),
                                ..Default::default()
                            },
                            ..Default::default()
                        },
                        dependencies: vec![],
                    },
                )?;
                store.queue_task(
                    RequestId::new(),
                    identity,
                    TaskVersion {
                        task_id: id,
                        revision: 1,
                    },
                )?;
            }
            Ok((identity, run_id))
        })
        .await
        .unwrap()
}

#[tokio::test]
async fn ready_runs_share_slots_and_acceptance_does_not_wait_for_native_startup() {
    let dir = tempfile::Builder::new()
        .prefix("adroit-scheduler-")
        .tempdir_in("/tmp")
        .unwrap();
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(dir.path(), std::fs::Permissions::from_mode(0o700)).unwrap();
    let source = dir.path().join("source");
    std::fs::create_dir(&source).unwrap();
    let root = dir.path().join("control");
    let service = adroit::control::service::Service::bind(&root)
        .await
        .unwrap();
    let actor = service.actor();
    let (_, first) = planned(&actor, &source, 3).await;
    let (_, second) = planned(&actor, &source, 1).await;
    let socket = dir.path().join("felis.sock");
    let daemon_task = support::serve_process(socket.clone());
    for _ in 0..100 {
        if socket.exists() {
            break;
        }
        assert!(
            !daemon_task.is_finished(),
            "daemon failed: {:?}",
            daemon_task.await
        );
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
    }
    let daemon = Daemon::new(socket);
    let mut settings = Settings::default();
    settings.codex.command = vec![
        "/bin/sh".into(),
        "-c".into(),
        support::codex_script(
            r#"printf '{"op":"snapshot"}' | "$ADROIT_TEST_BINARY" control > "observed-$ADROIT_SESSION_ID.json"; printf '%s' "$ADROIT_SESSION_ID" > started; sleep 10"#,
            None,
        ),
        "worker".into(),
    ];
    settings.codex.env.insert(
        "ADROIT_TEST_BINARY".into(),
        env!("CARGO_BIN_EXE_adroit").into(),
    );
    let mut scheduler = Scheduler::new(actor.clone(), daemon.clone(), root, "/adroit".into());
    let dispatched = scheduler.tick(settings.clone(), 2).await.unwrap();
    assert_eq!(dispatched.len(), 2);
    let snapshot = actor.call(|store| store.snapshot()).await.unwrap();
    let runs: std::collections::HashSet<_> = snapshot
        .sessions
        .iter()
        .filter(|s| s.binding.role() == SessionRole::Worker)
        .filter_map(|s| s.binding.run_id())
        .collect();
    assert_eq!(runs, [first, second].into_iter().collect());
    assert!(scheduler.tick(settings, 2).await.unwrap().is_empty());
    tokio::time::timeout(std::time::Duration::from_secs(10), async {
        loop {
            let snapshot = actor.call(|store| store.snapshot()).await.unwrap();
            if snapshot.attempts.iter().all(|attempt| {
                attempt.state == AttemptState::Running
                    && source.join("started").exists()
                    && std::fs::read(source.join(format!("observed-{}.json", attempt.session_id)))
                        .ok()
                        .and_then(|bytes| serde_json::from_slice::<serde_json::Value>(&bytes).ok())
                        .is_some()
            }) {
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        }
    })
    .await
    .expect("Workers must finish their first scoped control request");
    let snapshot = actor.call(|store| store.snapshot()).await.unwrap();
    assert_eq!(snapshot.attempts.len(), 2);
    assert!(
        snapshot
            .attempts
            .iter()
            .all(|attempt| attempt.state == AttemptState::Running),
        "{:?}",
        actor
            .call(|store| store.events_after(0, 100))
            .await
            .unwrap()
    );
    assert!(
        snapshot
            .attempts
            .iter()
            .all(|attempt| attempt.target.model.as_deref() == Some("test-small"))
    );
    assert_eq!(snapshot.work_areas.len(), 1);
    for attempt in &snapshot.attempts {
        let path = source.join(format!("observed-{}.json", attempt.session_id));
        let observed: serde_json::Value =
            serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap();
        let sessions = observed["result"]["Ok"]["sessions"].as_array().unwrap();
        assert!(
            sessions
                .iter()
                .any(
                    |session| session["binding"]["logical_id"] == attempt.session_id.to_string()
                        && session["state"] == "running"
                )
        );
    }
    assert!(source.join("started").exists());
    assert!(!source.join(".git").exists());
    assert!(!source.join(".jj").exists());
    for session in snapshot.sessions {
        if let Some(id) = session.terminal_id {
            assert_eq!(
                daemon
                    .visible(u128::from_str_radix(&id, 16).unwrap())
                    .await
                    .unwrap()
                    .len(),
                40
            );
            daemon
                .end(u128::from_str_radix(&id, 16).unwrap())
                .await
                .unwrap();
        }
    }
    daemon_task.abort();
}

#[tokio::test]
async fn failed_native_preparation_reports_failure_without_starting_in_the_source() {
    let dir = tempfile::tempdir().unwrap();
    let source = dir.path().join("source");
    std::fs::create_dir_all(source.join(".jj")).unwrap();
    let root = dir.path().join("control");
    let actor = Actor::open(root.join("state.sqlite3")).await.unwrap();
    let (owner, _) = planned(&actor, &source, 1).await;
    let mut scheduler = Scheduler::new(
        actor.clone(),
        Daemon::new(dir.path().join("absent.sock")),
        root,
        "/adroit".into(),
    );
    assert_eq!(
        scheduler.tick(Settings::default(), 1).await.unwrap().len(),
        1
    );
    for _ in 0..200 {
        let snapshot = actor.call(|store| store.snapshot()).await.unwrap();
        if snapshot.attempts[0].state == AttemptState::Failed {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
    }
    let snapshot = actor.call(|store| store.snapshot()).await.unwrap();
    assert_eq!(snapshot.attempts[0].state, AttemptState::Failed);
    assert_eq!(snapshot.tasks[0].state, TaskState::Failed);
    assert!(snapshot.work_areas.is_empty());
    let mailbox = actor
        .call(move |store| store.mailbox(owner, 0, 10))
        .await
        .unwrap();
    assert_eq!(mailbox.messages.len(), 1);
    assert!(mailbox.messages[0].message.body.contains("startup failed"));
    assert!(!source.join("started").exists());
}

#[tokio::test]
async fn one_invalid_provider_profile_does_not_block_another_run() {
    let dir = tempfile::tempdir().unwrap();
    let source = dir.path().join("source");
    std::fs::create_dir_all(source.join(".jj")).unwrap();
    let root = dir.path().join("control");
    let actor = Actor::open(root.join("state.sqlite3")).await.unwrap();
    let (owner, first) = planned(&actor, &source, 1).await;
    actor
        .call(move |store| {
            let task = store
                .snapshot()?
                .tasks
                .into_iter()
                .find(|task| task.run_id == first)
                .unwrap();
            let changed = store
                .put_task(
                    RequestId::new(),
                    owner,
                    TaskPlan {
                        id: task.id,
                        run_id: first,
                        expected_revision: Some(1),
                        brief: TaskBrief {
                            target: TargetPreference {
                                agent: Some(Agent::Claude),
                                model: None,
                                permissions: AgentPermissions::ReadOnly,
                            },
                            ..task.brief
                        },
                        dependencies: vec![],
                    },
                )?
                .value;
            store.queue_task(
                RequestId::new(),
                owner,
                TaskVersion {
                    task_id: task.id,
                    revision: changed.revision,
                },
            )?;
            Ok(())
        })
        .await
        .unwrap();
    let (_, second) = planned(&actor, &source, 1).await;
    let mut scheduler = Scheduler::new(
        actor.clone(),
        Daemon::new(dir.path().join("absent.sock")),
        root,
        "/adroit".into(),
    );
    scheduler.tick(Settings::default(), 1).await.unwrap();
    let snapshot = actor.call(|store| store.snapshot()).await.unwrap();
    assert_eq!(snapshot.attempts.len(), 2);
    let second_task = snapshot
        .tasks
        .iter()
        .find(|task| task.run_id == second)
        .unwrap();
    assert!(second_task.current_attempt.is_some());
    assert_eq!(
        snapshot
            .tasks
            .iter()
            .find(|task| task.run_id == first)
            .unwrap()
            .state,
        TaskState::Failed
    );
}

#[tokio::test]
async fn per_run_capacity_leaves_other_leaders_room_without_locking_a_shared_directory() {
    use std::os::unix::fs::PermissionsExt;
    let dir = tempfile::tempdir().unwrap();
    std::fs::set_permissions(dir.path(), std::fs::Permissions::from_mode(0o700)).unwrap();
    let source = dir.path().join("source");
    std::fs::create_dir(&source).unwrap();
    let root = dir.path().join("control");
    let service = adroit::control::service::Service::bind(&root)
        .await
        .unwrap();
    let actor = service.actor();
    let (_, first) = planned(&actor, &source, 3).await;
    let (_, second) = planned(&actor, &source, 3).await;
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
    let mut settings = Settings::default();
    settings.codex.command = vec![
        "/bin/sh".into(),
        "-c".into(),
        support::codex_script("sleep 10", None),
        "native-provider".into(),
    ];
    settings.orchestration.max_workers_per_run = 1;
    let mut scheduler = Scheduler::new(actor.clone(), daemon.clone(), root, "/adroit".into());
    assert_eq!(scheduler.tick(settings.clone(), 4).await.unwrap().len(), 2);
    assert!(
        scheduler
            .tick(settings.clone(), 4)
            .await
            .unwrap()
            .is_empty()
    );
    settings.orchestration.max_workers_per_run = 2;
    assert_eq!(scheduler.tick(settings, 4).await.unwrap().len(), 2);
    tokio::time::timeout(std::time::Duration::from_secs(5), async {
        loop {
            let snapshot = actor.call(|store| store.snapshot()).await.unwrap();
            if snapshot
                .attempts
                .iter()
                .all(|attempt| attempt.state == AttemptState::Running)
            {
                assert_eq!(snapshot.attempts.len(), 4);
                for run in [first, second] {
                    assert_eq!(
                        snapshot
                            .attempts
                            .iter()
                            .filter(|attempt| snapshot
                                .tasks
                                .iter()
                                .any(|task| task.id == attempt.task_id && task.run_id == run))
                            .count(),
                        2
                    );
                }
                assert_eq!(snapshot.work_areas.len(), 1);
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        }
    })
    .await
    .unwrap();
    for session in daemon.sessions().await.unwrap() {
        daemon.end(session.id).await.unwrap();
    }
    native.abort();
}
