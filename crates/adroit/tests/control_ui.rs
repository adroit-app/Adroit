use adroit::control::{protocol::*, store::Store};
use adroit_model::*;

#[test]
fn ui_pages_preserve_current_rows_and_attention_without_retransmitting_history() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("control/state.sqlite3");
    let mut store = Store::open(&path).unwrap();
    let owner = store
        .register_leader(
            RequestId::new(),
            LogicalSessionId::new(),
            Agent::Codex,
            dir.path().into(),
        )
        .unwrap()
        .value
        .reference();
    let run_id = RunId::new();
    store
        .open_run(
            RequestId::new(),
            owner,
            run_id,
            RunBrief {
                title: "Current request".into(),
                goal: "Keep live work visible".into(),
                ..Default::default()
            },
        )
        .unwrap();
    let task_id = TaskId::new();
    store
        .put_task(
            RequestId::new(),
            owner,
            TaskPlan {
                id: task_id,
                run_id,
                expected_revision: None,
                brief: TaskBrief {
                    instructions: format!("Keep this label\n{}", "large context ".repeat(5000)),
                    ..Default::default()
                },
                dependencies: vec![],
            },
        )
        .unwrap();
    let db = rusqlite::Connection::open(&path).unwrap();
    for n in 0..160 {
        let message = AgentMessage {
            id: MessageId::new(),
            run_id,
            from: owner.id,
            to: owner.id,
            task_id: Some(task_id),
            attempt_id: None,
            revision: Some(1),
            kind: if n == 159 {
                MessageKind::Question
            } else {
                MessageKind::Report
            },
            body: "full durable evidence ".repeat(600),
            reply_to: None,
            blocks_work: n == 159,
        };
        db.execute(
            "INSERT INTO messages(id,run_id,sender,recipient,data) VALUES(?1,?2,?3,?3,?4)",
            rusqlite::params![
                message.id.to_string(),
                run_id.to_string(),
                owner.id.to_string(),
                serde_json::to_string(&message).unwrap()
            ],
        )
        .unwrap();
    }
    assert!(
        serde_json::to_vec(&store.snapshot().unwrap())
            .unwrap()
            .len()
            > MAX_FRAME
    );
    let mut after = 0;
    let mut epoch = None;
    let mut messages = vec![];
    loop {
        let page = store.ui_page(UiTable::Messages, after, epoch).unwrap();
        assert!(serde_json::to_vec(&page).unwrap().len() < 300_000);
        epoch = Some(page.epoch);
        messages.extend(page.rows);
        let Some(next) = page.next_cursor else { break };
        assert!(next > after);
        after = next;
    }
    assert_eq!(messages.len(), 160);
    assert_eq!(messages.last().unwrap()["blocks_work"], true);
    let page = store.ui_page(UiTable::Tasks, 0, epoch).unwrap();
    let task: TaskRecord = serde_json::from_value(page.rows[0].clone()).unwrap();
    assert!(task.brief.instructions.starts_with("Keep this label\n"));
    assert!(task.brief.instructions.len() < 2048);
    let mut offset = 0;
    let mut text = String::new();
    loop {
        let page = store
            .ui_detail(UiTable::Tasks, task_id.to_string(), offset, epoch)
            .unwrap();
        text.push_str(&page.text);
        let Some(next) = page.next_offset else { break };
        offset = next;
    }
    let full: TaskRecord = serde_json::from_str(&text).unwrap();
    assert!(full.brief.instructions.len() > 60_000);
    assert!(store.ui_page(UiTable::Tasks, u64::MAX, epoch).is_err());
    assert!(store.ui_page(UiTable::Tasks, 1, None).is_err());
    assert!(
        store
            .ui_detail(UiTable::Tasks, task_id.to_string(), 1, None)
            .is_err()
    );
    assert!(
        store
            .ui_detail(UiTable::Tasks, task_id.to_string(), u32::MAX, epoch)
            .is_err()
    );
    store
        .put_task(
            RequestId::new(),
            owner,
            TaskPlan {
                id: task_id,
                run_id,
                expected_revision: Some(1),
                brief: TaskBrief {
                    instructions: "Updated task".into(),
                    ..Default::default()
                },
                dependencies: vec![],
            },
        )
        .unwrap();
    assert!(store.ui_page(UiTable::Tasks, 0, epoch).is_err());
    assert!(
        store
            .ui_detail(UiTable::Tasks, task_id.to_string(), 0, epoch)
            .is_err()
    );
    assert_eq!(store.snapshot().unwrap().messages.len(), 160);
    let first_line = format!("  A long exact label {}", "🦀  ".repeat(1000));
    store
        .put_task(
            RequestId::new(),
            owner,
            TaskPlan {
                id: task_id,
                run_id,
                expected_revision: Some(2),
                brief: TaskBrief {
                    instructions: format!("\n{first_line}\n{}", "full details ".repeat(5000)),
                    ..Default::default()
                },
                dependencies: vec![],
            },
        )
        .unwrap();
    let projected: TaskRecord = serde_json::from_value(
        store
            .ui_page(UiTable::Tasks, 0, None)
            .unwrap()
            .rows
            .remove(0),
    )
    .unwrap();
    assert_eq!(
        projected.brief.instructions.lines().next().unwrap(),
        first_line.split_whitespace().collect::<Vec<_>>().join(" ")
    );
}

fn database(root: &std::path::Path) -> Store {
    Store::open(&root.join("state.sqlite3")).unwrap()
}

#[tokio::test]
async fn human_projection_is_authenticated_and_selected_evidence_is_chunked() {
    use adroit::control::{client::Client, service::Service};
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().join("control");
    let service = Service::bind(&root).await.unwrap();
    let human = Client::human(&root).unwrap();
    let session = service
        .actor()
        .call(|store| {
            store.register_leader(
                RequestId::new(),
                LogicalSessionId::new(),
                Agent::Codex,
                "/tmp".into(),
            )
        })
        .await
        .unwrap()
        .value;
    let grant = adroit::control::client::AppClient::new(&root)
        .request(
            RequestId::new(),
            Command::Grant {
                identity: session.reference(),
            },
        )
        .await
        .unwrap();
    let agent = Client::session(&root, grant.as_str().unwrap().into());
    assert!(
        agent
            .request(
                RequestId::new(),
                Command::UiPage {
                    table: UiTable::Sessions,
                    after: 0,
                    epoch: None
                }
            )
            .await
            .is_err()
    );
    assert!(agent.ui_snapshot().await.is_err());
    assert!(
        human
            .clone()
            .as_actor(session.reference())
            .ui_snapshot()
            .await
            .is_err()
    );
    assert_eq!(human.ui_snapshot().await.unwrap().sessions.len(), 1);
    let run_id = RunId::new();
    let task_id = TaskId::new();
    let report_id = MessageId::new();
    let attempt_id = AttemptId::new();
    let report = TaskReportRecord {
        run_id,
        applied: true,
        report: TaskReport {
            message_id: report_id,
            task: TaskVersion {
                task_id,
                revision: 7,
            },
            attempt_id,
            result: TaskResult {
                status: ReportStatus::Succeeded,
                summary: "Current applied result".into(),
                artifact: Some(TaskArtifact::Report {
                    data: serde_json::json!({"evidence": "🦀".repeat(400_000)}),
                    files: vec![],
                    reviewed_revision: None,
                }),
                checks: vec![],
                unresolved: vec!["Needs human attention".into()],
            },
        },
    };
    let db = rusqlite::Connection::open(root.join("state.sqlite3")).unwrap();
    let run = RunRecord {
        id: run_id,
        leader_id: session.reference().id,
        brief: RunBrief {
            title: "Evidence".into(),
            goal: "Keep the full result".into(),
            ..Default::default()
        },
        plan_revision: 1,
        candidate_id: None,
        state: RunState::Active,
    };
    db.execute(
        "INSERT INTO runs(id,leader_id,state,data) VALUES(?1,?2,'active',?3)",
        rusqlite::params![
            run_id.to_string(),
            session.reference().id.to_string(),
            serde_json::to_string(&run).unwrap()
        ],
    )
    .unwrap();
    let task = TaskRecord {
        id: task_id,
        run_id,
        revision: 7,
        brief: TaskBrief {
            instructions: "Current report task".into(),
            ..Default::default()
        },
        dependencies: vec![],
        state: TaskState::Succeeded,
        current_attempt: Some(attempt_id),
    };
    db.execute(
        "INSERT INTO tasks(id,run_id,data) VALUES(?1,?2,?3)",
        rusqlite::params![
            task_id.to_string(),
            run_id.to_string(),
            serde_json::to_string(&task).unwrap()
        ],
    )
    .unwrap();
    let attempt = AttemptRecord {
        id: attempt_id,
        task_id,
        task_revision: 7,
        input_versions: vec![],
        session_id: session.reference().id,
        target: TargetSelection::default(),
        actual_model: None,
        candidate_input: None,
        work_area_id: None,
        base_revision: None,
        state: AttemptState::Succeeded,
        readiness: WorkerReadiness::Ready,
        spawned_at_ms: None,
    };
    db.execute(
        "INSERT INTO attempts(id,task_id,session_id,active,data) VALUES(?1,?2,?3,0,?4)",
        rusqlite::params![
            attempt_id.to_string(),
            task_id.to_string(),
            session.reference().id.to_string(),
            serde_json::to_string(&attempt).unwrap()
        ],
    )
    .unwrap();
    let message = AgentMessage {
        id: report_id,
        run_id,
        from: session.reference().id,
        to: session.reference().id,
        task_id: Some(task_id),
        attempt_id: Some(attempt_id),
        revision: Some(7),
        kind: MessageKind::Report,
        body: serde_json::to_string(&report.report.result).unwrap(),
        reply_to: None,
        blocks_work: false,
    };
    db.execute(
        "INSERT INTO messages(id,run_id,sender,recipient,data) VALUES(?1,?2,?3,?3,?4)",
        rusqlite::params![
            report_id.to_string(),
            run_id.to_string(),
            session.reference().id.to_string(),
            serde_json::to_string(&message).unwrap()
        ],
    )
    .unwrap();
    db.execute(
        "INSERT INTO task_reports(id,attempt_id,data) VALUES(?1,?2,?3)",
        rusqlite::params![
            report_id.to_string(),
            attempt_id.to_string(),
            serde_json::to_string(&report).unwrap()
        ],
    )
    .unwrap();
    let oversized = human
        .request(RequestId::new(), Command::Snapshot)
        .await
        .unwrap_err();
    assert_eq!(
        oversized.message,
        "the response exceeds the control frame limit"
    );
    let snapshot = human.ui_snapshot().await.unwrap();
    assert_eq!(snapshot.reports[0].report.task.revision, 7);
    assert!(snapshot.reports[0].applied);
    assert_eq!(
        snapshot.reports[0].report.result.unresolved,
        vec!["Needs human attention"]
    );
    assert_eq!(
        snapshot.tasks[0].brief.instructions.lines().next().unwrap(),
        task.brief.instructions
    );
    let full = human
        .ui_detail(UiTable::Reports, report_id.to_string())
        .await
        .unwrap();
    assert_eq!(
        serde_json::from_str::<TaskReportRecord>(&full).unwrap(),
        report
    );
    db.execute(
        "UPDATE tasks SET data=json_set(data, '$.current_attempt', NULL) WHERE id=?1",
        [task_id.to_string()],
    )
    .unwrap();
    assert!(human.ui_snapshot().await.unwrap().reports.is_empty());
    let history = human.ui_task_history(task_id, None).await.unwrap();
    assert_eq!(history.len(), 1);
    assert_eq!(history[0].attempt_id, attempt_id);
    assert_eq!(history[0].session_id, session.reference().id);
    assert_eq!(history[0].task_revision, 7);
    let discovered_id = history[0].report.as_ref().unwrap().id;
    assert_eq!(discovered_id, report_id);
    assert_eq!(
        serde_json::from_str::<TaskReportRecord>(
            &human
                .ui_detail(UiTable::Reports, discovered_id.to_string())
                .await
                .unwrap()
        )
        .unwrap(),
        report
    );
    assert!(
        human
            .ui_detail(UiTable::Reports, MessageId::new().to_string())
            .await
            .is_err()
    );
    let mut replacement = attempt.clone();
    replacement.id = AttemptId::new();
    replacement.task_revision = 8;
    // More than one history page, with stable insertion order and no bodies.
    let mut expected = vec![attempt_id];
    for _ in 0..65 {
        replacement.id = AttemptId::new();
        replacement.session_id = LogicalSessionId::new();
        let mut worker = session.clone();
        worker.binding = SessionBinding::worker(
            replacement.session_id,
            session.reference().id,
            run_id,
            task_id,
            replacement.id,
        )
        .unwrap();
        db.execute(
            "INSERT INTO logical_sessions(id,data) VALUES(?1,?2)",
            rusqlite::params![
                replacement.session_id.to_string(),
                serde_json::to_string(&worker).unwrap()
            ],
        )
        .unwrap();
        db.execute(
            "INSERT INTO attempts(id,task_id,session_id,active,data) VALUES(?1,?2,?3,0,?4)",
            rusqlite::params![
                replacement.id.to_string(),
                task_id.to_string(),
                replacement.session_id.to_string(),
                serde_json::to_string(&replacement).unwrap()
            ],
        )
        .unwrap();
        expected.push(replacement.id);
    }
    db.execute("UPDATE tasks SET data=json_set(data, '$.current_attempt', ?2, '$.revision', 8) WHERE id=?1", rusqlite::params![task_id.to_string(), replacement.id.to_string()]).unwrap();
    let view = human.ui_snapshot().await.unwrap();
    let history = human
        .ui_task_history(task_id, Some(view.event_cursor))
        .await
        .unwrap();
    assert_eq!(
        history.iter().map(|h| h.attempt_id).collect::<Vec<_>>(),
        expected
    );
    assert!(history[0].is_historical(&view.tasks[0]));
    assert!(!history.last().unwrap().is_historical(&view.tasks[0]));
    assert_eq!(view.runs[0].leader_id, session.reference().id);
    assert_eq!(history[0].session_id, session.reference().id);
    assert!(serde_json::to_vec(&history).unwrap().len() < 32_000);
    assert!(agent.ui_task_history(task_id, None).await.is_err());
    assert!(
        human
            .clone()
            .as_actor(session.reference())
            .ui_task_history(task_id, None)
            .await
            .is_err()
    );
    assert!(
        agent
            .request(
                RequestId::new(),
                Command::UiTaskHistory {
                    task_id,
                    after: 0,
                    epoch: None
                }
            )
            .await
            .is_err()
    );
    assert!(
        agent
            .request(
                RequestId::new(),
                Command::UiRecord {
                    table: UiTable::Tasks,
                    rowid: 1,
                    offset: 0,
                    epoch: view.event_cursor
                }
            )
            .await
            .is_err()
    );
    // Replace only this disposable test endpoint with an old-protocol fixture.
    drop(service);
    let legacy = legacy_control(&root);
    assert_eq!(
        human
            .ui_task_history(task_id, Some(view.event_cursor))
            .await
            .unwrap(),
        history
    );
    let discovered = history[0].report.as_ref().unwrap().id;
    assert_eq!(
        serde_json::from_str::<TaskReportRecord>(
            &human
                .ui_detail(UiTable::Reports, discovered.to_string())
                .await
                .unwrap()
        )
        .unwrap(),
        report
    );
    legacy.abort();
}

#[tokio::test]
async fn legacy_fallback_requires_explicit_incompatibility_and_never_migrates_the_database() {
    use adroit::control::{
        client::Client,
        store::{ErrorCode, ui::read_only_snapshot},
    };
    use std::os::unix::fs::PermissionsExt;
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().join("control");
    let _store = database(&root);
    let token = "a".repeat(72);
    std::fs::write(root.join("human.token"), &token).unwrap();
    std::fs::set_permissions(
        root.join("human.token"),
        std::fs::Permissions::from_mode(0o600),
    )
    .unwrap();
    assert!(read_only_snapshot(&root, "incorrect").is_err());
    let client = Client::human(&root).unwrap();
    assert!(
        client.ui_snapshot().await.is_err(),
        "missing daemon must not activate fallback"
    );
    for message in [
        "invalid control request",
        "the response exceeds the control frame limit",
        "credential rejected",
    ] {
        let listener = tokio::net::UnixListener::bind(root.join("control.sock")).unwrap();
        let message = message.to_owned();
        let expected_ok = message != "credential rejected";
        let server = tokio::spawn(async move {
            use tokio::io::{AsyncBufReadExt, AsyncWriteExt};
            let (stream, _) = listener.accept().await.unwrap();
            let (read, mut write) = stream.into_split();
            let mut line = String::new();
            tokio::io::BufReader::new(read)
                .read_line(&mut line)
                .await
                .unwrap();
            let request: Request = serde_json::from_str(&line).unwrap();
            let response = Response {
                version: VERSION,
                id: if message == "invalid control request" {
                    None
                } else {
                    Some(request.id)
                },
                result: Err(adroit::control::store::ControlError::new(
                    ErrorCode::InvalidRequest,
                    message,
                )),
            };
            let mut bytes = serde_json::to_vec(&response).unwrap();
            bytes.push(b'\n');
            write.write_all(&bytes).await.unwrap();
        });
        assert_eq!(client.ui_snapshot().await.is_ok(), expected_ok);
        server.await.unwrap();
        std::fs::remove_file(root.join("control.sock")).unwrap();
    }
    let db = rusqlite::Connection::open(root.join("state.sqlite3")).unwrap();
    db.pragma_update(None, "user_version", 999).unwrap();
    assert_eq!(
        read_only_snapshot(&root, &token).unwrap_err().code,
        ErrorCode::UnsupportedSchema
    );
    assert_eq!(
        db.pragma_query_value(None, "user_version", |row| row.get::<_, i64>(0))
            .unwrap(),
        999
    );
    db.pragma_update(None, "user_version", 12).unwrap();
    std::fs::set_permissions(
        root.join("state.sqlite3"),
        std::fs::Permissions::from_mode(0o644),
    )
    .unwrap();
    assert!(read_only_snapshot(&root, &token).is_err());
}

#[tokio::test]
async fn legacy_ui_recovery_keeps_native_terminals_in_the_same_daemon_alive() {
    use adroit::control::client::Client;
    use adroit_term::{Daemon, Spawn};
    use std::{os::unix::fs::PermissionsExt, process::Stdio, time::Duration};
    let dir = tempfile::Builder::new()
        .prefix("adroit-ui-")
        .tempdir_in("/tmp")
        .unwrap();
    let root = dir.path().join("control");
    let mut store = database(&root);
    let owner = store
        .register_leader(
            RequestId::new(),
            LogicalSessionId::new(),
            Agent::Codex,
            dir.path().into(),
        )
        .unwrap()
        .value
        .reference();
    let run_id = RunId::new();
    store
        .open_run(
            RequestId::new(),
            owner,
            run_id,
            RunBrief {
                title: "Legacy large metadata".into(),
                goal: "Keep native terminal".into(),
                ..Default::default()
            },
        )
        .unwrap();
    let mut plans = vec![];
    for n in 0..4 {
        let plan = large_task(run_id, n);
        store
            .put_task(RequestId::new(), owner, plan.clone())
            .unwrap();
        plans.push(plan);
    }
    std::fs::write(root.join("human.token"), "a".repeat(72)).unwrap();
    std::fs::set_permissions(
        root.join("human.token"),
        std::fs::Permissions::from_mode(0o600),
    )
    .unwrap();
    let before = std::fs::read(root.join("state.sqlite3")).unwrap();
    let mut child = tokio::process::Command::new(std::env::current_exe().unwrap())
        .args([
            "--exact",
            "legacy_daemon_fixture",
            "--ignored",
            "--nocapture",
        ])
        .env("ADROIT_TEST_UI_LEGACY_ROOT", &root)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::inherit())
        .kill_on_drop(true)
        .spawn()
        .unwrap();
    let daemon = Daemon::new(root.join("d.sock"));
    let mut ready = false;
    for _ in 0..200 {
        if daemon.sessions().await.is_ok() && root.join("control.sock").exists() {
            ready = true;
            break;
        }
        assert!(child.try_wait().unwrap().is_none(), "fixture exited");
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    assert!(ready, "fixture did not start");
    let terminal = daemon
        .spawn(Spawn {
            program: "/bin/sh".into(),
            args: vec!["-c".into(), "printf preserved; sleep 30".into()],
            cwd: "/tmp".into(),
            cols: 40,
            rows: 6,
            ..Default::default()
        })
        .await
        .unwrap();
    let human = Client::human(&root).unwrap();
    for _ in 0..3 {
        let view = human.ui_snapshot().await.unwrap();
        assert_eq!(view.sessions.len(), 1);
        assert_eq!(view.tasks.len(), plans.len());
        for (task, plan) in view.tasks.iter().zip(&plans) {
            assert_eq!(
                task.brief.instructions.lines().next().unwrap(),
                plan.brief.instructions
            );
            assert_eq!(task.brief.scope, plan.brief.scope);
            assert_eq!(task.brief.inputs, plan.brief.inputs);
            assert_eq!(task.state, TaskState::Planned);
        }
        assert!(
            daemon
                .sessions()
                .await
                .unwrap()
                .iter()
                .any(|session| session.id == terminal && !session.exited)
        );
    }
    assert_eq!(std::fs::read(root.join("state.sqlite3")).unwrap(), before);
    assert!(child.try_wait().unwrap().is_none());
    daemon.end(terminal).await.unwrap();
    child.kill().await.unwrap();
}

#[tokio::test]
#[ignore = "subprocess fixture isolates native daemon socket permissions"]
async fn legacy_daemon_fixture() {
    use tokio::io::{AsyncBufReadExt, AsyncWriteExt};
    let root = std::path::PathBuf::from(std::env::var_os("ADROIT_TEST_UI_LEGACY_ROOT").unwrap());
    let listener = tokio::net::UnixListener::bind(root.join("control.sock")).unwrap();
    tokio::spawn(async move {
        loop {
            let (stream, _) = listener.accept().await.unwrap();
            tokio::spawn(async move {
                let (read, mut write) = stream.into_split();
                let mut line = String::new();
                tokio::io::BufReader::new(read)
                    .read_line(&mut line)
                    .await
                    .unwrap();
                // The old serde enum rejects the UI command before identifying it.
                let response = Response {
                    version: VERSION,
                    id: None,
                    result: Err(adroit::control::store::ControlError::new(
                        adroit::control::store::ErrorCode::InvalidRequest,
                        "invalid control request",
                    )),
                };
                let mut bytes = serde_json::to_vec(&response).unwrap();
                bytes.push(b'\n');
                write.write_all(&bytes).await.unwrap();
            });
        }
    });
    adroit_term::Daemon::serve(
        &root.join("d.sock"),
        std::time::Duration::from_millis(20),
        |_| {},
    )
    .await
    .unwrap();
}

#[test]
fn compact_attention_uses_complete_history_before_paging_and_keeps_answer_evidence() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().join("control");
    let mut store = database(&root);
    let owner = store
        .register_leader(
            RequestId::new(),
            LogicalSessionId::new(),
            Agent::Codex,
            "/tmp".into(),
        )
        .unwrap()
        .value
        .reference();
    let run = RunId::new();
    store
        .open_run(
            RequestId::new(),
            owner,
            run,
            RunBrief {
                title: "Attention".into(),
                goal: "Preserve completion decisions".into(),
                ..Default::default()
            },
        )
        .unwrap();
    let db = rusqlite::Connection::open(root.join("state.sqlite3")).unwrap();
    let insert = |kind, reply_to, acknowledged| {
        let message = AgentMessage {
            id: MessageId::new(),
            run_id: run,
            from: owner.id,
            to: owner.id,
            task_id: None,
            attempt_id: None,
            revision: None,
            kind,
            body: "durable body ".repeat(1000),
            reply_to,
            blocks_work: false,
        };
        db.execute(
            "INSERT INTO messages(id,run_id,sender,recipient,data) VALUES(?1,?2,?3,?3,?4)",
            rusqlite::params![
                message.id.to_string(),
                run.to_string(),
                owner.id.to_string(),
                serde_json::to_string(&message).unwrap()
            ],
        )
        .unwrap();
        let delivery = DeliveryRecord {
            message_id: message.id,
            recipient: owner.id,
            state: if acknowledged {
                DeliveryState::Acknowledged
            } else {
                DeliveryState::Stored
            },
            attempts: 1,
            last_error: None,
        };
        db.execute(
            "INSERT INTO deliveries(message_id,recipient,data) VALUES(?1,?2,?3)",
            rusqlite::params![
                message.id.to_string(),
                owner.id.to_string(),
                serde_json::to_string(&delivery).unwrap()
            ],
        )
        .unwrap();
        message.id
    };
    let answered = insert(MessageKind::Question, None, true);
    insert(MessageKind::Answer, Some(answered), true);
    for _ in 0..150 {
        insert(MessageKind::Report, None, true);
    }
    let mut rows = vec![];
    let mut after = 0;
    loop {
        let page = store.ui_page(UiTable::Messages, after, None).unwrap();
        rows.extend(page.rows);
        match page.next_cursor {
            Some(next) => after = next,
            None => break,
        }
    }
    assert!(
        rows.is_empty(),
        "acknowledged history has no remaining attention"
    );
    let pending = insert(MessageKind::Question, None, false);
    let answer = insert(MessageKind::Answer, Some(pending), true);
    let unanswered = insert(MessageKind::Question, None, true);
    let note = insert(MessageKind::Note, None, false);
    let page = store.ui_page(UiTable::Messages, 0, None).unwrap();
    let messages = page
        .rows
        .into_iter()
        .map(|row| serde_json::from_value::<AgentMessage>(row).unwrap())
        .collect::<Vec<_>>();
    assert_eq!(
        messages.iter().map(|m| m.id).collect::<Vec<_>>(),
        vec![pending, answer, unanswered, note]
    );
    let unanswered_ids = |messages: &[AgentMessage]| {
        messages
            .iter()
            .filter(|q| {
                q.kind == MessageKind::Question
                    && !messages.iter().any(|a| {
                        a.kind == MessageKind::Answer
                            && a.reply_to == Some(q.id)
                            && a.from == q.to
                            && a.to == q.from
                    })
            })
            .map(|q| q.id)
            .collect::<Vec<_>>()
    };
    let full = store.snapshot().unwrap();
    assert!(serde_json::to_vec(&full).unwrap().len() > MAX_FRAME);
    assert_eq!(unanswered_ids(&messages), unanswered_ids(&full.messages));
    assert_eq!(unanswered_ids(&messages), vec![unanswered]);
    assert_eq!(
        store
            .ui_page(UiTable::Deliveries, 0, None)
            .unwrap()
            .rows
            .len(),
        4
    );
}

fn large_task(run_id: RunId, n: usize) -> TaskPlan {
    TaskPlan {
        id: TaskId::new(),
        run_id,
        expected_revision: None,
        brief: TaskBrief {
            instructions: match n {
                0 => "x".repeat(300 * 1024),
                1 => "Large semantic vectors".into(),
                _ => format!("{n}{}", "🦀".repeat(300 * 1024 / 4)),
            },
            // Escaping expands these bytes on the wire; these are semantic fields.
            scope: if n == 0 {
                vec![]
            } else {
                vec!["\"\\\u{0}🦀".repeat(36_000)]
            },
            inputs: if n == 0 {
                vec![]
            } else {
                vec!["input".repeat(1000); 8]
            },
            ..Default::default()
        },
        dependencies: vec![],
    }
}

#[tokio::test]
async fn supported_large_task_requests_refresh_atomically_through_the_service() {
    use adroit::control::{client::Client, service::Service};
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().join("control");
    let service = Service::bind(&root).await.unwrap();
    let session = service
        .actor()
        .call(|store| {
            store.register_leader(
                RequestId::new(),
                LogicalSessionId::new(),
                Agent::Codex,
                "/tmp".into(),
            )
        })
        .await
        .unwrap()
        .value;
    let human = Client::human(&root).unwrap();
    let actor = adroit::control::client::AppClient::new(&root).as_actor(session.reference());
    let run_id = RunId::new();
    actor
        .request(
            RequestId::new(),
            Command::OpenRun {
                id: run_id,
                brief: RunBrief {
                    title: "Large supported metadata".into(),
                    goal: "Preserve exact labels and fields".into(),
                    ..Default::default()
                },
            },
        )
        .await
        .unwrap();
    let mut plans = vec![];
    for n in 0..4 {
        let plan = large_task(run_id, n);
        actor
            .request(RequestId::new(), Command::PutTask { plan: plan.clone() })
            .await
            .unwrap();
        plans.push(plan);
    }
    let view = human.ui_snapshot().await.unwrap();
    assert!(serde_json::to_vec(&view).unwrap().len() > MAX_FRAME);
    // Inspect actual service responses, including the envelope and newline.
    let mut after = 0;
    let epoch = view.event_cursor;
    loop {
        let id = RequestId::new();
        let value = human
            .request(
                id,
                Command::UiPage {
                    table: UiTable::Tasks,
                    after,
                    epoch: Some(epoch),
                },
            )
            .await
            .unwrap();
        assert_wire_budget(id, &value);
        let page: UiPage = serde_json::from_value(value).unwrap();
        if let Some(reference) = page.record {
            let mut text = String::new();
            while (text.len() as u64) < reference.bytes {
                let id = RequestId::new();
                let value = human
                    .request(
                        id,
                        Command::UiRecord {
                            table: UiTable::Tasks,
                            rowid: reference.rowid,
                            offset: text.len() as u64,
                            epoch,
                        },
                    )
                    .await
                    .unwrap();
                assert_wire_budget(id, &value);
                let chunk: UiRecordChunk = serde_json::from_value(value).unwrap();
                assert_eq!(chunk.offset, text.len() as u64);
                assert_eq!(chunk.rowid, reference.rowid);
                assert_eq!(chunk.epoch, epoch);
                assert!(!chunk.text.is_empty());
                text.push_str(&chunk.text);
            }
            assert_eq!(text.len() as u64, reference.bytes);
            let task: TaskRecord = serde_json::from_str(&text).unwrap();
            assert!(plans.iter().any(|plan| plan.id == task.id));
        }
        match page.next_cursor {
            Some(next) => {
                assert!(next > after);
                after = next;
            }
            None => break,
        }
    }
    // A mutation invalidates every outstanding record fragment, too.
    let mut update = plans[0].clone();
    update.expected_revision = Some(1);
    actor
        .request(RequestId::new(), Command::PutTask { plan: update })
        .await
        .unwrap();
    let error = human
        .request(
            RequestId::new(),
            Command::UiRecord {
                table: UiTable::Tasks,
                rowid: 1,
                offset: 32_768,
                epoch,
            },
        )
        .await
        .unwrap_err();
    assert_eq!(
        error.code,
        adroit::control::store::ErrorCode::RevisionConflict
    );
    assert_eq!(view.tasks.len(), plans.len());
    for (task, plan) in view.tasks.iter().zip(plans) {
        assert_eq!(task.id, plan.id);
        assert_eq!(
            task.brief.instructions.lines().next().unwrap(),
            plan.brief.instructions
        );
        assert_eq!(task.brief.scope, plan.brief.scope);
        assert_eq!(task.brief.inputs, plan.brief.inputs);
        assert_eq!(task.state, TaskState::Planned);
        assert_eq!(task.run_id, run_id);
    }
}

fn legacy_control(root: &std::path::Path) -> tokio::task::JoinHandle<()> {
    use tokio::io::{AsyncBufReadExt, AsyncWriteExt};
    let listener = tokio::net::UnixListener::bind(root.join("control.sock")).unwrap();
    tokio::spawn(async move {
        loop {
            let (stream, _) = listener.accept().await.unwrap();
            tokio::spawn(async move {
                let (read, mut write) = stream.into_split();
                let mut line = String::new();
                tokio::io::BufReader::new(read)
                    .read_line(&mut line)
                    .await
                    .unwrap();
                let response = Response {
                    version: VERSION,
                    id: None,
                    result: Err(adroit::control::store::ControlError::new(
                        adroit::control::store::ErrorCode::InvalidRequest,
                        "invalid control request",
                    )),
                };
                let mut bytes = serde_json::to_vec(&response).unwrap();
                bytes.push(b'\n');
                write.write_all(&bytes).await.unwrap();
            });
        }
    })
}

fn assert_wire_budget(id: RequestId, value: &serde_json::Value) {
    let mut bytes = serde_json::to_vec(&Response {
        version: VERSION,
        id: Some(id),
        result: Ok(value.clone()),
    })
    .unwrap();
    bytes.push(b'\n');
    assert!(bytes.len() <= MAX_FRAME);
    let _: Response = serde_json::from_slice(&bytes).unwrap();
}

#[tokio::test]
async fn an_inconsistent_record_fragment_cannot_publish_a_partial_snapshot() {
    use adroit::control::client::Client;
    use tokio::io::{AsyncBufReadExt, AsyncWriteExt};
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("human.token"), "a".repeat(72)).unwrap();
    let listener = tokio::net::UnixListener::bind(dir.path().join("control.sock")).unwrap();
    let server = tokio::spawn(async move {
        for n in 0..2 {
            let (stream, _) = listener.accept().await.unwrap();
            let (read, mut write) = stream.into_split();
            let mut line = String::new();
            tokio::io::BufReader::new(read)
                .read_line(&mut line)
                .await
                .unwrap();
            let request: Request = serde_json::from_str(&line).unwrap();
            let value = if n == 0 {
                assert!(matches!(request.command, Command::UiPage { after: 0, .. }));
                serde_json::to_value(UiPage {
                    epoch: 1,
                    next_cursor: Some(1),
                    rows: vec![],
                    record: Some(UiRecordRef {
                        rowid: 1,
                        bytes: 10,
                    }),
                })
                .unwrap()
            } else {
                assert!(matches!(
                    request.command,
                    Command::UiRecord {
                        rowid: 1,
                        offset: 0,
                        epoch: 1,
                        ..
                    }
                ));
                serde_json::to_value(UiRecordChunk {
                    epoch: 1,
                    rowid: 1,
                    offset: 1,
                    text: "{}".into(),
                })
                .unwrap()
            };
            let mut bytes = serde_json::to_vec(&Response {
                version: VERSION,
                id: Some(request.id),
                result: Ok(value),
            })
            .unwrap();
            bytes.push(b'\n');
            write.write_all(&bytes).await.unwrap();
        }
    });
    let error = Client::human(dir.path())
        .unwrap()
        .ui_snapshot()
        .await
        .unwrap_err();
    assert!(error.message.contains("inconsistent UI record fragment"));
    server.await.unwrap();
}
