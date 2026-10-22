mod support;
use adroit::control::{actor::Actor, scheduler::Scheduler};
use adroit::settings::Settings;
use adroit_model::*;
use adroit_term::Daemon;

async fn planned(actor: &Actor, source: &std::path::Path, count: usize) -> (SessionRef, RunId) {
    planned_brief(actor, source, count, TaskBrief::default()).await
}

async fn planned_brief(
    actor: &Actor,
    source: &std::path::Path,
    count: usize,
    brief: TaskBrief,
) -> (SessionRef, RunId) {
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
                                permissions: brief.target.permissions.clone(),
                            },
                            ..brief.clone()
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

fn vcs(source: &std::path::Path, program: &str, args: &[&str]) -> String {
    let output = std::process::Command::new(program)
        .args(args)
        .current_dir(source)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout).unwrap()
}

#[tokio::test]
async fn read_only_auto_workers_share_live_sources_without_native_preparation() {
    for backend in [
        WorkAreaBackend::Jj,
        WorkAreaBackend::Git,
        WorkAreaBackend::Directory,
    ] {
        let dir = tempfile::tempdir().unwrap();
        let source = dir.path().join("source");
        std::fs::create_dir(&source).unwrap();
        match backend {
            WorkAreaBackend::Jj => {
                vcs(&source, "jj", &["git", "init", "--colocate"]);
            }
            WorkAreaBackend::Git => {
                vcs(&source, "git", &["init"]);
            }
            WorkAreaBackend::Directory => {}
        }
        if backend != WorkAreaBackend::Directory {
            if backend == WorkAreaBackend::Git {
                // Use jj to write fixture history, then retain the Git checkout alone.
                vcs(&source, "jj", &["git", "init", "--colocate"]);
            }
            vcs(
                &source,
                "jj",
                &[
                    "--config",
                    "user.name=Test",
                    "--config",
                    "user.email=test@example.invalid",
                    "describe",
                    "-m",
                    "test: prepare live source",
                ],
            );
            std::fs::write(source.join("tracked"), "recorded input").unwrap();
            vcs(
                &source,
                "jj",
                &[
                    "--config",
                    "user.name=Test",
                    "--config",
                    "user.email=test@example.invalid",
                    "new",
                    "-m",
                    "test: change live input",
                ],
            );
            if backend == WorkAreaBackend::Git {
                std::fs::rename(source.join(".jj"), dir.path().join("jj-metadata")).unwrap();
            }
        }
        std::fs::write(source.join("tracked"), "dirty input").unwrap();
        std::fs::write(source.join("untracked"), "live input").unwrap();
        let before = (backend == WorkAreaBackend::Jj).then(|| {
            vcs(
                &source,
                "jj",
                &[
                    "--ignore-working-copy",
                    "op",
                    "log",
                    "--no-graph",
                    "-T",
                    "id",
                    "-n",
                    "1",
                ],
            )
        });
        let actor = Actor::open(dir.path().join("control/state.sqlite3"))
            .await
            .unwrap();
        let brief = TaskBrief {
            target: TargetPreference {
                permissions: AgentPermissions::ReadOnly,
                ..Default::default()
            },
            ..Default::default()
        };
        planned_brief(&actor, &source, 2, brief.clone()).await;
        planned_brief(&actor, &source, 1, brief).await;
        let mut scheduler = Scheduler::new(
            actor.clone(),
            Daemon::new(dir.path().join("absent.sock")),
            dir.path().join("control"),
            "/adroit".into(),
        );
        scheduler.tick(Settings::default(), 3).await.unwrap();
        let snapshot = settled_preparation(&actor).await;
        assert_eq!(snapshot.work_areas.len(), 1, "{backend:?}");
        let area = &snapshot.work_areas[0];
        assert_eq!(area.path, source.canonicalize().unwrap(), "{backend:?}");
        assert_eq!(area.backend, backend);
        assert_eq!(area.state, WorkAreaState::Ready);
        assert_eq!(area.base_revision, None);
        assert_eq!(snapshot.run_work_areas.len(), 2);
        assert!(
            snapshot
                .attempts
                .iter()
                .all(|a| a.work_area_id == Some(area.id)
                    && a.target.permissions == AgentPermissions::ReadOnly)
        );
        assert_eq!(
            std::fs::read_to_string(source.join("untracked")).unwrap(),
            "live input"
        );
        if let Some(before) = before {
            assert_eq!(
                vcs(
                    &source,
                    "jj",
                    &[
                        "--ignore-working-copy",
                        "op",
                        "log",
                        "--no-graph",
                        "-T",
                        "id",
                        "-n",
                        "1"
                    ]
                ),
                before
            );
            assert_eq!(
                vcs(
                    &source,
                    "jj",
                    &["--ignore-working-copy", "workspace", "list"]
                )
                .lines()
                .count(),
                1
            );
        }
        assert_eq!(
            std::fs::read_to_string(source.join("tracked")).unwrap(),
            "dirty input"
        );
        assert!(!source.join(".claude/worktrees").exists());
    }
}

async fn settled_preparation(actor: &Actor) -> ControlSnapshot {
    tokio::time::timeout(std::time::Duration::from_secs(10), async {
        loop {
            let snapshot = actor.call(|store| store.snapshot()).await.unwrap();
            if snapshot.attempts.iter().all(|a| {
                a.state == AttemptState::Failed
                    || a.work_area_id.is_some_and(|id| {
                        snapshot
                            .work_areas
                            .iter()
                            .any(|area| area.id == id && area.state == WorkAreaState::Ready)
                    })
            }) {
                return snapshot;
            }
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        }
    })
    .await
    .unwrap()
}

#[tokio::test]
async fn only_unpinned_recorded_read_only_permissions_reuse_native_sources() {
    for (permissions, pinned, reuse) in [
        (AgentPermissions::ReadOnly, false, true),
        (AgentPermissions::ReadOnly, true, false),
        (AgentPermissions::Default, false, false),
        (
            AgentPermissions::Custom(Box::new(CustomPermissions::Codex {
                approval_policy: "never".into(),
                sandbox_mode: "read-only".into(),
            })),
            false,
            false,
        ),
    ] {
        let dir = tempfile::tempdir().unwrap();
        let source = dir.path().join("source");
        std::fs::create_dir(&source).unwrap();
        vcs(&source, "jj", &["git", "init", "--colocate"]);
        let base = vcs(
            &source,
            "jj",
            &[
                "--ignore-working-copy",
                "log",
                "-r",
                "@",
                "--no-graph",
                "-T",
                "commit_id",
            ],
        );
        std::fs::write(source.join("untracked"), "live").unwrap();
        let actor = Actor::open(dir.path().join("control/state.sqlite3"))
            .await
            .unwrap();
        planned_brief(
            &actor,
            &source,
            1,
            TaskBrief {
                base_revision: pinned.then_some(base.clone()),
                target: TargetPreference {
                    permissions: permissions.clone(),
                    ..Default::default()
                },
                purpose: TaskPurpose::Review,
                ..Default::default()
            },
        )
        .await;
        let mut scheduler = Scheduler::new(
            actor.clone(),
            Daemon::new(dir.path().join("absent.sock")),
            dir.path().join("control"),
            "/adroit".into(),
        );
        scheduler.tick(Settings::default(), 1).await.unwrap();
        let snapshot = settled_preparation(&actor).await;
        assert_eq!(snapshot.work_areas.len(), 1);
        let area = &snapshot.work_areas[0];
        assert_eq!(
            area.path == source.canonicalize().unwrap(),
            reuse,
            "{permissions:?}, pinned={pinned}"
        );
        assert_eq!(snapshot.attempts[0].target.permissions, permissions);
        if pinned {
            assert_eq!(area.base_revision.as_deref(), Some(base.as_str()));
            assert!(!area.path.join("untracked").exists());
        }
    }
}

#[tokio::test]
async fn explicit_source_and_existing_area_take_precedence_for_read_only_tasks() {
    for existing in [false, true] {
        let dir = tempfile::tempdir().unwrap();
        let leader_source = dir.path().join("leader");
        let selected = dir.path().join("selected");
        let explicit = dir.path().join("existing");
        for path in [&leader_source, &selected, &explicit] {
            std::fs::create_dir(path).unwrap();
        }
        let actor = Actor::open(dir.path().join("control/state.sqlite3"))
            .await
            .unwrap();
        let area = adroit::workareas::existing(
            WorkAreaId::new(),
            explicit.clone(),
            "Explicit area".into(),
        )
        .await
        .unwrap();
        let (owner, run) = planned_brief(
            &actor,
            &leader_source,
            1,
            TaskBrief {
                source_dir: Some(selected.clone()),
                workdir: if existing {
                    WorkdirPolicy::Existing(area.id)
                } else {
                    WorkdirPolicy::Auto
                },
                target: TargetPreference {
                    permissions: AgentPermissions::ReadOnly,
                    ..Default::default()
                },
                ..Default::default()
            },
        )
        .await;
        if existing {
            actor
                .call(move |store| {
                    store
                        .register_work_area(RequestId::new(), owner, run, area, false)
                        .map(|_| ())
                })
                .await
                .unwrap();
        }
        let mut scheduler = Scheduler::new(
            actor.clone(),
            Daemon::new(dir.path().join("absent.sock")),
            dir.path().join("control"),
            "/adroit".into(),
        );
        scheduler.tick(Settings::default(), 1).await.unwrap();
        let snapshot = settled_preparation(&actor).await;
        assert_eq!(snapshot.work_areas.len(), 1);
        assert_eq!(
            snapshot.work_areas[0].path,
            if existing { explicit } else { selected }
                .canonicalize()
                .unwrap()
        );
    }
}

#[tokio::test]
async fn read_only_source_identification_errors_do_not_create_or_bind_fallback_areas() {
    for marker in [".jj", ".git", "missing"] {
        let dir = tempfile::tempdir().unwrap();
        let source = dir.path().join("source");
        std::fs::create_dir(&source).unwrap();
        if marker != "missing" {
            std::fs::create_dir(source.join(marker)).unwrap();
        }
        let selected = if marker == "missing" {
            source.join("missing")
        } else {
            source.clone()
        };
        let actor = Actor::open(dir.path().join("control/state.sqlite3"))
            .await
            .unwrap();
        planned_brief(
            &actor,
            &source,
            1,
            TaskBrief {
                source_dir: Some(selected),
                target: TargetPreference {
                    permissions: AgentPermissions::ReadOnly,
                    ..Default::default()
                },
                ..Default::default()
            },
        )
        .await;
        let mut scheduler = Scheduler::new(
            actor.clone(),
            Daemon::new(dir.path().join("absent.sock")),
            dir.path().join("control"),
            "/adroit".into(),
        );
        scheduler.tick(Settings::default(), 1).await.unwrap();
        let snapshot = settled_preparation(&actor).await;
        assert!(snapshot.work_areas.is_empty(), "{marker}");
        assert!(snapshot.attempts[0].work_area_id.is_none());
        assert!(source.exists());
    }
}

#[tokio::test]
async fn an_unreadable_run_keeps_its_worker_slots_and_leaves_other_runs_scheduled() {
    let dir = tempfile::tempdir().unwrap();
    let source = dir.path().join("source");
    std::fs::create_dir_all(&source).unwrap();
    let root = dir.path().join("control");
    let actor = Actor::open(root.join("state.sqlite3")).await.unwrap();
    let (spoiled_owner, spoiled) = planned(&actor, &source, 2).await;
    let (_, kept) = planned(&actor, &source, 1).await;
    let (running, waiting) = actor
        .call(move |store| {
            let tasks: Vec<_> = store
                .snapshot()?
                .tasks
                .into_iter()
                .filter(|task| task.run_id == spoiled)
                .map(|task| task.id)
                .collect();
            let version = TaskVersion {
                task_id: tasks[0],
                revision: 1,
            };
            let target = TargetSelection {
                agent: Agent::Codex,
                ..Default::default()
            };
            store.reserve_attempt(RequestId::new(), spoiled_owner, version, target)?;
            Ok((tasks[0], tasks[1]))
        })
        .await
        .unwrap();
    assert_ne!(running, waiting);
    rusqlite::Connection::open(root.join("state.sqlite3"))
        .unwrap()
        .execute(
            "UPDATE tasks SET data = json_set(data, '$.unknown_field', 1) WHERE id = ?1",
            [waiting.to_string()],
        )
        .unwrap();
    let mut scheduler = Scheduler::new(
        actor.clone(),
        Daemon::new(dir.path().join("absent.sock")),
        root,
        "/adroit".into(),
    );

    let full = scheduler.tick(Settings::default(), 1).await.unwrap();
    let held: Vec<_> = scheduler
        .unreadable()
        .iter()
        .map(|run| run.run_id)
        .collect();
    let launched = scheduler.tick(Settings::default(), 2).await.unwrap();

    assert!(full.is_empty(), "{full:?}");
    assert_eq!(held, vec![spoiled]);
    assert_eq!(launched.len(), 1);
    let snapshot = actor
        .call(|store| store.readable_snapshot())
        .await
        .unwrap()
        .snapshot;
    let attempt = snapshot
        .attempts
        .iter()
        .find(|attempt| attempt.id == launched[0])
        .unwrap();
    let task = snapshot
        .tasks
        .iter()
        .find(|task| task.id == attempt.task_id)
        .unwrap();
    assert_eq!(task.run_id, kept);
}
