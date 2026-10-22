use adroit::control::store::Store;
use adroit_model::{
    Agent, AgentMessage, AgentPermissions, DeliveryState, LogicalSessionId, MessageId, MessageKind,
    ModelSource, RequestId, RunBrief, RunId, SessionRef, TargetSelection, TaskBrief, TaskId,
    TaskPlan, TaskVersion,
};
use std::path::Path;

#[test]
fn independent_run_creation_requires_an_exact_human_grant_and_retries_once() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("control/state.sqlite3");
    let mut store = Store::open(&path).unwrap();
    let (owner, original) = leader(&mut store, dir.path());
    let brief = RunBrief {
        title: "Separate request".into(),
        goal: "Investigate independently".into(),
        ..Default::default()
    };
    let grant = store
        .authorize_independent_run(
            RequestId::new(),
            adroit::control::store::IndependentAuthorizationPlan {
                leader: owner,
                workspace: dir.path().into(),
                brief: brief.clone(),
                agent: Agent::Claude,
                model: None,
                permissions: AgentPermissions::Default,
            },
        )
        .unwrap()
        .value;
    let worker_task = task(&mut store, owner, original, vec![]);
    let version = TaskVersion {
        task_id: worker_task,
        revision: 1,
    };
    store.queue_task(RequestId::new(), owner, version).unwrap();
    let worker = store
        .reserve_attempt(RequestId::new(), owner, version, target())
        .unwrap()
        .value;
    assert!(
        store
            .create_independent_run(
                RequestId::new(),
                SessionRef {
                    id: worker.session_id,
                    generation: 1
                },
                grant.id
            )
            .is_err()
    );
    let request = RequestId::new();
    let created = store
        .create_independent_run(request, owner, grant.id)
        .unwrap()
        .value;
    assert_ne!(created.run_id, original);
    assert_ne!(created.leader_id, owner.id);
    drop(store);
    let mut store = Store::open(&path).unwrap();
    assert_eq!(
        store
            .create_independent_run(request, owner, grant.id)
            .unwrap()
            .value,
        created
    );
    assert!(
        store
            .create_independent_run(RequestId::new(), owner, grant.id)
            .is_err()
    );
    let snapshot = store.snapshot().unwrap();
    assert_eq!(
        snapshot
            .runs
            .iter()
            .filter(|run| run.id == created.run_id)
            .count(),
        1
    );
    assert_eq!(
        snapshot
            .sessions
            .iter()
            .find(|session| session.binding.logical_id() == created.leader_id)
            .unwrap()
            .cwd
            .as_deref(),
        Some(dir.path().canonicalize().unwrap().as_path())
    );
}

fn leader(store: &mut Store, cwd: &Path) -> (SessionRef, RunId) {
    let id = LogicalSessionId::new();
    let session = store
        .register_leader(RequestId::new(), id, Agent::Claude, cwd.into())
        .unwrap()
        .value;
    let owner = SessionRef {
        id,
        generation: session.generation,
    };
    let run = RunId::new();
    store
        .open_run(
            RequestId::new(),
            owner,
            run,
            RunBrief {
                title: "Improve the project".into(),
                goal: "Complete the requested change".into(),
                ..Default::default()
            },
        )
        .unwrap();
    (owner, run)
}

fn task(store: &mut Store, owner: SessionRef, run: RunId, dependencies: Vec<TaskId>) -> TaskId {
    let id = TaskId::new();
    store
        .put_task(
            RequestId::new(),
            owner,
            TaskPlan {
                id,
                run_id: run,
                expected_revision: None,
                brief: TaskBrief {
                    instructions: "Review the change".into(),
                    ..Default::default()
                },
                dependencies,
            },
        )
        .unwrap();
    id
}

fn target() -> TargetSelection {
    TargetSelection {
        agent: Agent::Codex,
        requested_model: None,
        model: None,
        model_source: ModelSource::AgentDefault,
        permissions: AgentPermissions::ReadOnly,
    }
}

fn directory_area(path: &Path) -> adroit_model::WorkArea {
    let path = path.canonicalize().unwrap();
    adroit_model::WorkArea {
        id: adroit_model::WorkAreaId::new(),
        backend: adroit_model::WorkAreaBackend::Directory,
        path: path.clone(),
        source: path.clone(),
        source_root: path,
        repository: None,
        base_revision: None,
        native_name: None,
        description: "Shared source directory".into(),
        state: adroit_model::WorkAreaState::Preparing,
        error: None,
    }
}

fn pending_report(store: &mut Store, cwd: &Path) -> (SessionRef, AgentMessage) {
    let (owner, run) = leader(store, cwd);
    let task = task(store, owner, run, vec![]);
    let version = TaskVersion {
        task_id: task,
        revision: 1,
    };
    store.queue_task(RequestId::new(), owner, version).unwrap();
    let attempt = store
        .reserve_attempt(RequestId::new(), owner, version, target())
        .unwrap()
        .value;
    let message = AgentMessage {
        id: MessageId::new(),
        run_id: run,
        from: attempt.session_id,
        to: owner.id,
        task_id: Some(task),
        attempt_id: Some(attempt.id),
        revision: Some(1),
        kind: MessageKind::Report,
        body: "Review completed".into(),
        reply_to: None,
        blocks_work: false,
    };
    store
        .send_message(
            RequestId::new(),
            SessionRef {
                id: attempt.session_id,
                generation: 1,
            },
            message.clone(),
        )
        .unwrap();
    (owner, message)
}

#[test]
fn transport_acceptance_records_receipt_and_explicit_acknowledgement_is_idempotent() {
    let dir = tempfile::tempdir().unwrap();
    let mut store = Store::open(&dir.path().join("control/state.sqlite3")).unwrap();
    let (owner, message) = pending_report(&mut store, dir.path());
    let delivery = store
        .accept_delivery(RequestId::new(), message.id, owner)
        .unwrap()
        .value;
    assert_eq!(delivery.state, DeliveryState::Acknowledged);
    let acknowledgment = store
        .acknowledge_message(RequestId::new(), owner, message.id)
        .unwrap()
        .value;
    assert_eq!(acknowledgment.state, DeliveryState::Acknowledged);
}

#[test]
fn an_old_generation_cannot_accept_delivery_after_a_leader_restart() {
    let dir = tempfile::tempdir().unwrap();
    let mut store = Store::open(&dir.path().join("control/state.sqlite3")).unwrap();
    let (owner, message) = pending_report(&mut store, dir.path());
    store
        .remember_launch(
            RequestId::new(),
            owner,
            adroit::control::runtime::RuntimeLaunch::new(adroit::settings::Settings::default()),
        )
        .unwrap();
    let terminal = "0123456789abcdef0123456789abcdef".to_owned();
    store
        .attach_session(
            RequestId::new(),
            owner,
            dir.path().into(),
            terminal.clone(),
            MessageId::new().to_string(),
        )
        .unwrap();
    store
        .claim_delivery(RequestId::new(), owner, message.id)
        .unwrap();
    store
        .observe_terminal_exit(RequestId::new(), owner, terminal)
        .unwrap();
    let replacement = store
        .restart_leader(RequestId::new(), owner)
        .unwrap()
        .value
        .reference();
    assert!(
        store
            .accept_delivery(RequestId::new(), message.id, owner)
            .is_err()
    );
    assert_eq!(
        store.mailbox(replacement, 0, 100).unwrap().messages[0]
            .delivery
            .state,
        DeliveryState::Uncertain
    );
    store
        .retry_delivery(RequestId::new(), replacement, message.id)
        .unwrap();
    assert!(
        store
            .accept_delivery(RequestId::new(), message.id, owner)
            .is_err()
    );
    assert_eq!(
        store.mailbox(replacement, 0, 100).unwrap().messages[0]
            .delivery
            .state,
        DeliveryState::Stored
    );
}

#[test]
fn a_late_acceptance_cannot_downgrade_an_acknowledged_message() {
    let dir = tempfile::tempdir().unwrap();
    let mut store = Store::open(&dir.path().join("control/state.sqlite3")).unwrap();
    let (owner, message) = pending_report(&mut store, dir.path());
    store
        .acknowledge_message(RequestId::new(), owner, message.id)
        .unwrap();
    let delivery = store
        .accept_delivery(RequestId::new(), message.id, owner)
        .unwrap()
        .value;
    assert_eq!(delivery.state, DeliveryState::Acknowledged);
    assert_eq!(
        store.snapshot().unwrap().deliveries[0].state,
        DeliveryState::Acknowledged
    );
}

#[test]
fn replaying_a_message_with_another_request_id_does_not_queue_it_twice() {
    let dir = tempfile::tempdir().unwrap();
    let mut store = Store::open(&dir.path().join("control/state.sqlite3")).unwrap();
    let (_, message) = pending_report(&mut store, dir.path());
    let before = store.snapshot().unwrap().event_cursor;
    store
        .send_message(
            RequestId::new(),
            SessionRef {
                id: message.from,
                generation: 1,
            },
            message,
        )
        .unwrap();
    assert_eq!(store.snapshot().unwrap().deliveries.len(), 1);
    assert!(store.events_after(before, 100).unwrap().is_empty());
}

#[test]
fn a_repeated_request_returns_the_saved_response_without_a_second_run_or_event() {
    let dir = tempfile::tempdir().unwrap();
    let mut store = Store::open(&dir.path().join("control/state.sqlite3")).unwrap();
    let id = LogicalSessionId::new();
    let request = RequestId::new();
    let first = store
        .register_leader(request, id, Agent::Claude, dir.path().into())
        .unwrap();
    let second = store
        .register_leader(request, id, Agent::Claude, dir.path().into())
        .unwrap();

    assert_eq!(first, second);
    assert_eq!(store.snapshot().unwrap().sessions.len(), 1);
    assert_eq!(store.events_after(0, 100).unwrap().len(), 1);
    assert!(
        store
            .register_leader(request, id, Agent::Codex, dir.path().into())
            .is_err()
    );
}

#[test]
fn a_report_and_its_pending_delivery_survive_reopening_the_store() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("control/state.sqlite3");
    let mut store = Store::open(&path).unwrap();
    let (owner, run) = leader(&mut store, dir.path());
    let task = task(&mut store, owner, run, vec![]);
    let version = TaskVersion {
        task_id: task,
        revision: 1,
    };
    store.queue_task(RequestId::new(), owner, version).unwrap();
    let attempt = store
        .reserve_attempt(RequestId::new(), owner, version, target())
        .unwrap()
        .value;
    let message = AgentMessage {
        id: MessageId::new(),
        run_id: run,
        from: attempt.session_id,
        to: owner.id,
        task_id: Some(task),
        attempt_id: Some(attempt.id),
        revision: Some(1),
        kind: MessageKind::Report,
        body: "Review completed".into(),
        reply_to: None,
        blocks_work: false,
    };
    let sent = store
        .send_message(
            RequestId::new(),
            SessionRef {
                id: attempt.session_id,
                generation: 1,
            },
            message.clone(),
        )
        .unwrap();
    drop(store);
    let mut store = Store::open(&path).unwrap();
    let snapshot = store.snapshot().unwrap();
    assert_eq!(snapshot.runs.len(), 1);
    assert_eq!(snapshot.tasks.len(), 1);
    assert_eq!(snapshot.attempts.len(), 1);
    assert_eq!(snapshot.messages, [message]);
    assert_eq!(snapshot.deliveries[0].state, DeliveryState::Stored);
    assert_eq!(snapshot.event_cursor, sent.event_cursor);
    assert_eq!(snapshot.attempts[0].target.agent, Agent::Codex);
}

#[test]
fn failing_to_assign_a_foreign_task_rolls_back_state_and_notifications() {
    let dir = tempfile::tempdir().unwrap();
    let mut store = Store::open(&dir.path().join("control/state.sqlite3")).unwrap();
    let (first, first_run) = leader(&mut store, dir.path());
    let (second, _) = leader(&mut store, dir.path());
    let task = task(&mut store, first, first_run, vec![]);
    let before = store.snapshot().unwrap();
    assert!(
        store
            .queue_task(
                RequestId::new(),
                second,
                TaskVersion {
                    task_id: task,
                    revision: 1
                }
            )
            .is_err()
    );
    assert_eq!(store.snapshot().unwrap(), before);
}

#[test]
fn task_dependencies_cannot_cross_runs_or_form_cycles() {
    let dir = tempfile::tempdir().unwrap();
    let mut store = Store::open(&dir.path().join("control/state.sqlite3")).unwrap();
    let (first, first_run) = leader(&mut store, dir.path());
    let (second, second_run) = leader(&mut store, dir.path());
    let a = task(&mut store, first, first_run, vec![]);
    let b = task(&mut store, first, first_run, vec![a]);
    let foreign = task(&mut store, second, second_run, vec![]);
    for dependencies in [vec![a], vec![b], vec![foreign]] {
        assert!(
            store
                .put_task(
                    RequestId::new(),
                    first,
                    TaskPlan {
                        id: a,
                        run_id: first_run,
                        expected_revision: Some(1),
                        brief: TaskBrief {
                            instructions: "Changed instructions".into(),
                            ..Default::default()
                        },
                        dependencies,
                    }
                )
                .is_err()
        );
    }
    assert_eq!(
        store
            .snapshot()
            .unwrap()
            .tasks
            .iter()
            .find(|t| t.id == a)
            .unwrap()
            .revision,
        1
    );
}

#[test]
fn an_attempt_is_reserved_once_and_old_task_revisions_cannot_start() {
    let dir = tempfile::tempdir().unwrap();
    let mut store = Store::open(&dir.path().join("control/state.sqlite3")).unwrap();
    let (owner, run) = leader(&mut store, dir.path());
    let task = task(&mut store, owner, run, vec![]);
    let version = TaskVersion {
        task_id: task,
        revision: 1,
    };
    store.queue_task(RequestId::new(), owner, version).unwrap();
    let request = RequestId::new();
    let first = store
        .reserve_attempt(request, owner, version, target())
        .unwrap();
    assert_eq!(
        store
            .reserve_attempt(request, owner, version, target())
            .unwrap(),
        first
    );
    assert!(
        store
            .reserve_attempt(RequestId::new(), owner, version, target())
            .is_err()
    );
    assert!(
        store
            .queue_task(
                RequestId::new(),
                owner,
                TaskVersion {
                    task_id: task,
                    revision: 0
                }
            )
            .is_err()
    );
    assert_eq!(store.snapshot().unwrap().attempts.len(), 1);
}

#[test]
fn a_corrupted_database_is_not_replaced_with_empty_state() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("control/state.sqlite3");
    drop(Store::open(&path).unwrap());
    std::fs::write(&path, b"This is not a SQLite database").unwrap();
    let before = std::fs::read(&path).unwrap();
    assert!(Store::open(&path).is_err());
    assert_eq!(std::fs::read(&path).unwrap(), before);
}

#[test]
fn a_worker_cannot_report_as_another_task_in_the_same_run() {
    let dir = tempfile::tempdir().unwrap();
    let mut store = Store::open(&dir.path().join("control/state.sqlite3")).unwrap();
    let (owner, run) = leader(&mut store, dir.path());
    let first = task(&mut store, owner, run, vec![]);
    let other = task(&mut store, owner, run, vec![]);
    let mut attempts = Vec::new();
    for task_id in [first, other] {
        let version = TaskVersion {
            task_id,
            revision: 1,
        };
        store.queue_task(RequestId::new(), owner, version).unwrap();
        attempts.push(
            store
                .reserve_attempt(RequestId::new(), owner, version, target())
                .unwrap()
                .value,
        );
    }
    let forged = AgentMessage {
        id: MessageId::new(),
        run_id: run,
        from: attempts[0].session_id,
        to: owner.id,
        task_id: Some(other),
        attempt_id: Some(attempts[1].id),
        revision: Some(1),
        kind: MessageKind::Report,
        body: "This report belongs to the other worker".into(),
        reply_to: None,
        blocks_work: false,
    };
    let sender = SessionRef {
        id: attempts[0].session_id,
        generation: 1,
    };
    assert!(
        store
            .send_message(RequestId::new(), sender, forged)
            .is_err()
    );
    assert!(store.snapshot().unwrap().messages.is_empty());
}

#[test]
fn a_newer_schema_is_refused_without_changing_its_version() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("control/state.sqlite3");
    drop(Store::open(&path).unwrap());
    let connection = rusqlite::Connection::open(&path).unwrap();
    connection.pragma_update(None, "user_version", 99).unwrap();
    drop(connection);
    let error = Store::open(&path).err().unwrap();
    assert_eq!(
        error.code,
        adroit::control::store::ErrorCode::UnsupportedSchema
    );
    let connection = rusqlite::Connection::open(&path).unwrap();
    assert_eq!(
        connection
            .pragma_query_value(None, "user_version", |row| row.get::<_, i64>(0))
            .unwrap(),
        99
    );
}

#[test]
fn opening_a_store_from_before_worker_readiness_raises_its_version_past_older_builds() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("control/state.sqlite3");
    drop(Store::open(&path).unwrap());
    let connection = rusqlite::Connection::open(&path).unwrap();
    connection.pragma_update(None, "user_version", 14).unwrap();

    drop(Store::open(&path).unwrap());

    assert_eq!(
        connection
            .pragma_query_value(None, "user_version", |row| row.get::<_, i64>(0))
            .unwrap(),
        15
    );
}

#[test]
fn an_open_store_stops_reading_and_writing_once_a_newer_build_upgrades_the_schema() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("control/state.sqlite3");
    let mut store = Store::open(&path).unwrap();
    leader(&mut store, dir.path());
    let newer = rusqlite::Connection::open(&path).unwrap();
    newer.pragma_update(None, "user_version", 99).unwrap();

    let read = store.snapshot().unwrap_err();
    let written = store
        .register_leader(
            RequestId::new(),
            LogicalSessionId::new(),
            Agent::Claude,
            dir.path().into(),
        )
        .unwrap_err();

    use adroit::control::store::ErrorCode;
    assert_eq!(read.code, ErrorCode::UnsupportedSchema);
    assert_eq!(written.code, ErrorCode::UnsupportedSchema);
    assert!(
        written.message.contains("newer adroit")
            && written.message.contains("restart the session daemon"),
        "{written}"
    );
    let sessions: i64 = newer
        .query_row("SELECT COUNT(*) FROM logical_sessions", [], |row| {
            row.get(0)
        })
        .unwrap();
    assert_eq!(sessions, 1);
}

#[tokio::test]
async fn the_database_owner_refuses_every_call_once_a_newer_build_upgrades_the_schema() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("control/state.sqlite3");
    let actor = adroit::control::actor::Actor::open(path.clone())
        .await
        .unwrap();
    rusqlite::Connection::open(&path)
        .unwrap()
        .pragma_update(None, "user_version", 99)
        .unwrap();

    let error = actor
        .call(|store| store.interrupted_startups())
        .await
        .unwrap_err();

    assert_eq!(
        error.code,
        adroit::control::store::ErrorCode::UnsupportedSchema
    );
}

#[test]
fn a_truncated_existing_store_is_not_treated_as_a_fresh_instance() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("control/state.sqlite3");
    let mut store = Store::open(&path).unwrap();
    leader(&mut store, dir.path());
    drop(store);
    std::fs::OpenOptions::new()
        .write(true)
        .truncate(true)
        .open(&path)
        .unwrap();
    assert!(Store::open(&path).is_err());
    assert_eq!(std::fs::metadata(&path).unwrap().len(), 0);
}

#[test]
fn attaching_a_started_worker_records_the_terminal_and_running_attempt_together() {
    let dir = tempfile::tempdir().unwrap();
    let mut store = Store::open(&dir.path().join("control/state.sqlite3")).unwrap();
    let (owner, run) = leader(&mut store, dir.path());
    let task = task(&mut store, owner, run, vec![]);
    let version = TaskVersion {
        task_id: task,
        revision: 1,
    };
    store.queue_task(RequestId::new(), owner, version).unwrap();
    let attempt = store
        .reserve_attempt(RequestId::new(), owner, version, target())
        .unwrap()
        .value;
    let identity = SessionRef {
        id: attempt.session_id,
        generation: 1,
    };
    assert!(
        store
            .attach_session(
                RequestId::new(),
                identity,
                dir.path().into(),
                "00000000000000000000000000000001".into(),
                "conversation".into()
            )
            .is_err()
    );
    let area = directory_area(dir.path());
    store
        .register_work_area(RequestId::new(), owner, run, area.clone(), false)
        .unwrap();
    store
        .bind_work_area(RequestId::new(), owner, version, attempt.id, area.id)
        .unwrap();
    store
        .finish_work_area(RequestId::new(), area.id, Ok(()))
        .unwrap();
    store
        .begin_start(RequestId::new(), owner, version, attempt.id)
        .unwrap();
    store
        .attach_session(
            RequestId::new(),
            identity,
            area.path.clone(),
            "00000000000000000000000000000001".into(),
            "conversation".into(),
        )
        .unwrap();
    let snapshot = store.snapshot().unwrap();
    let session = snapshot
        .sessions
        .iter()
        .find(|session| session.reference() == identity)
        .unwrap();
    assert_eq!(
        session.terminal_id.as_deref(),
        Some("00000000000000000000000000000001")
    );
    assert_eq!(session.state, adroit_model::ManagedSessionState::Running);
    assert_eq!(
        snapshot.attempts[0].state,
        adroit_model::AttemptState::Running
    );
    assert_eq!(snapshot.tasks[0].state, adroit_model::TaskState::Running);
    assert!(
        store
            .attach_session(
                RequestId::new(),
                identity,
                dir.path().into(),
                "00000000000000000000000000000002".into(),
                "another".into()
            )
            .is_err()
    );
}

#[test]
fn a_failed_spawn_releases_its_attempt_without_succeeding_the_task() {
    let dir = tempfile::tempdir().unwrap();
    let mut store = Store::open(&dir.path().join("control/state.sqlite3")).unwrap();
    let (owner, run) = leader(&mut store, dir.path());
    let task = task(&mut store, owner, run, vec![]);
    let version = TaskVersion {
        task_id: task,
        revision: 1,
    };
    store.queue_task(RequestId::new(), owner, version).unwrap();
    let attempt = store
        .reserve_attempt(RequestId::new(), owner, version, target())
        .unwrap()
        .value;
    store
        .fail_start(
            RequestId::new(),
            SessionRef {
                id: attempt.session_id,
                generation: 1,
            },
            "program not found".into(),
        )
        .unwrap();
    let snapshot = store.snapshot().unwrap();
    assert_eq!(
        snapshot.attempts[0].state,
        adroit_model::AttemptState::Failed
    );
    assert!(!snapshot.attempts[0].state.holds_slot());
    assert_eq!(snapshot.tasks[0].state, adroit_model::TaskState::Failed);
    assert_eq!(snapshot.runs[0].state, adroit_model::RunState::Active);
    let notifications = store.mailbox(owner, 0, 10).unwrap();
    assert_eq!(notifications.messages.len(), 1);
    assert!(
        notifications.messages[0]
            .message
            .body
            .contains("program not found")
    );
}

#[test]
fn mailbox_pages_keep_private_payloads_bounded_and_advance_without_skipping() {
    let dir = tempfile::tempdir().unwrap();
    let mut store = Store::open(&dir.path().join("control/state.sqlite3")).unwrap();
    let (owner, mut message) = pending_report(&mut store, dir.path());
    for _ in 0..4 {
        message.id = MessageId::new();
        message.body = "\u{1}".repeat(64 * 1024);
        store
            .send_message(
                RequestId::new(),
                SessionRef {
                    id: message.from,
                    generation: 1,
                },
                message.clone(),
            )
            .unwrap();
    }
    let page = store.mailbox(owner, 0, 100).unwrap();
    assert!(serde_json::to_vec(&page).unwrap().len() < 1_048_576);
    assert!(page.messages.len() < 5);
    let next = store.mailbox(owner, page.next_cursor, 100).unwrap();
    assert!(
        next.messages
            .iter()
            .all(|entry| entry.sequence > page.next_cursor)
    );
    message.id = MessageId::new();
    message.body.push('x');
    assert!(
        store
            .send_message(
                RequestId::new(),
                SessionRef {
                    id: message.from,
                    generation: 1
                },
                message
            )
            .is_err()
    );
}

#[test]
fn only_one_dispatcher_can_claim_a_pending_message_and_a_crash_leaves_uncertainty() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("control/state.sqlite3");
    let mut store = Store::open(&path).unwrap();
    let (owner, message) = pending_report(&mut store, dir.path());
    let first = store
        .claim_delivery(RequestId::new(), owner, message.id)
        .unwrap()
        .value
        .unwrap();
    assert_eq!(first.state, DeliveryState::Dispatching);
    assert_eq!(first.attempts, 1);
    assert!(
        store
            .claim_delivery(RequestId::new(), owner, message.id)
            .unwrap()
            .value
            .is_none()
    );
    drop(store);
    let mut store = Store::open(&path).unwrap();
    store.recover_deliveries().unwrap();
    assert_eq!(
        store.snapshot().unwrap().deliveries[0].state,
        DeliveryState::Uncertain
    );
    store
        .accept_delivery(RequestId::new(), message.id, owner)
        .unwrap();
    let delivered = &store.snapshot().unwrap().deliveries[0];
    assert_eq!(delivered.state, DeliveryState::Acknowledged);
    assert_eq!(delivered.attempts, 1);
}

#[test]
fn shared_directory_registration_is_identity_matching_without_serializing_runs() {
    use adroit_model::{WorkArea, WorkAreaBackend, WorkAreaId, WorkAreaState};
    let dir = tempfile::tempdir().unwrap();
    let source = dir.path().canonicalize().unwrap();
    let mut store = Store::open(&dir.path().join("control/state.sqlite3")).unwrap();
    let (first, run_a) = leader(&mut store, &source);
    let (second, run_b) = leader(&mut store, &source);
    let area = WorkArea {
        id: WorkAreaId::new(),
        backend: WorkAreaBackend::Directory,
        path: source.clone(),
        source: source.clone(),
        source_root: source,
        repository: None,
        base_revision: None,
        native_name: None,
        description: "Shared input directory".into(),
        state: WorkAreaState::Preparing,
        error: None,
    };
    let kept = store
        .register_work_area(RequestId::new(), first, run_a, area.clone(), true)
        .unwrap()
        .value;
    let mut another = area;
    another.id = WorkAreaId::new();
    let reused = store
        .register_work_area(RequestId::new(), second, run_b, another, true)
        .unwrap()
        .value;
    assert_eq!(kept.id, reused.id);
    assert_eq!(store.snapshot().unwrap().work_areas.len(), 1);
    assert_eq!(store.snapshot().unwrap().run_work_areas.len(), 2);
}

#[test]
fn a_worker_cannot_start_until_its_registered_working_area_is_ready() {
    use adroit_model::{WorkArea, WorkAreaBackend, WorkAreaId, WorkAreaState};
    let dir = tempfile::tempdir().unwrap();
    let source = dir.path().canonicalize().unwrap();
    let mut store = Store::open(&dir.path().join("control/state.sqlite3")).unwrap();
    let (owner, run) = leader(&mut store, &source);
    let task = task(&mut store, owner, run, vec![]);
    let version = TaskVersion {
        task_id: task,
        revision: 1,
    };
    store.queue_task(RequestId::new(), owner, version).unwrap();
    let attempt = store
        .reserve_attempt(RequestId::new(), owner, version, target())
        .unwrap()
        .value;
    let area = WorkArea {
        id: WorkAreaId::new(),
        backend: WorkAreaBackend::Directory,
        path: source.clone(),
        source: source.clone(),
        source_root: source,
        repository: None,
        base_revision: None,
        native_name: None,
        description: "Shared input directory".into(),
        state: WorkAreaState::Preparing,
        error: None,
    };
    store
        .register_work_area(RequestId::new(), owner, run, area.clone(), false)
        .unwrap();
    store
        .bind_work_area(RequestId::new(), owner, version, attempt.id, area.id)
        .unwrap();
    assert!(
        store
            .begin_start(RequestId::new(), owner, version, attempt.id)
            .is_err()
    );
    store
        .finish_work_area(RequestId::new(), area.id, Ok(()))
        .unwrap();
    let started = store
        .begin_start(RequestId::new(), owner, version, attempt.id)
        .unwrap()
        .value;
    assert_eq!(started.state, adroit_model::AttemptState::Starting);
    assert_eq!(started.work_area_id, Some(area.id));
    drop(store);
    let mut store = Store::open(&dir.path().join("control/state.sqlite3")).unwrap();
    assert_eq!(
        store.snapshot().unwrap().work_areas[0].state,
        WorkAreaState::Ready
    );
}

fn running_worker(
    store: &mut Store,
    cwd: &Path,
) -> (SessionRef, RunId, adroit_model::AttemptRecord) {
    let (owner, run) = leader(store, cwd);
    let id = task(store, owner, run, vec![]);
    let version = TaskVersion {
        task_id: id,
        revision: 1,
    };
    store.queue_task(RequestId::new(), owner, version).unwrap();
    let attempt = store
        .reserve_attempt(RequestId::new(), owner, version, target())
        .unwrap()
        .value;
    let area = store
        .register_work_area(RequestId::new(), owner, run, directory_area(cwd), true)
        .unwrap()
        .value;
    store
        .bind_work_area(RequestId::new(), owner, version, attempt.id, area.id)
        .unwrap();
    store
        .finish_work_area(RequestId::new(), area.id, Ok(()))
        .unwrap();
    store
        .begin_start(RequestId::new(), owner, version, attempt.id)
        .unwrap();
    store
        .attach_session(
            RequestId::new(),
            SessionRef {
                id: attempt.session_id,
                generation: 1,
            },
            cwd.canonicalize().unwrap(),
            "000000000000000000000000000000ab".into(),
            "worker-conversation".into(),
        )
        .unwrap();
    (owner, run, attempt)
}

fn successful_report(attempt: &adroit_model::AttemptRecord) -> adroit_model::TaskReport {
    serde_json::from_value(serde_json::json!({
        "message_id": MessageId::new(), "task": {"task_id": attempt.task_id, "revision": attempt.task_revision},
        "attempt_id": attempt.id,
        "result": {"status": "succeeded", "summary": "Reviewed the requested inputs", "artifact": {"kind":"report", "data": {"findings":[]}, "files":[], "reviewed_revision":null}, "checks":[], "unresolved":[]}
    })).unwrap()
}

#[test]
fn a_result_and_its_notification_are_committed_together() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("control/state.sqlite3");
    let mut store = Store::open(&path).unwrap();
    let (owner, run, attempt) = running_worker(&mut store, dir.path());
    let report = successful_report(&attempt);
    let worker = SessionRef {
        id: attempt.session_id,
        generation: 1,
    };
    let request = RequestId::new();
    let result = store.report_task(request, worker, report.clone()).unwrap();
    assert!(result.value.applied);
    assert_eq!(
        store.report_task(request, worker, report.clone()).unwrap(),
        result
    );
    drop(store);
    let mut store = Store::open(&path).unwrap();
    let snapshot = store.snapshot().unwrap();
    assert_eq!(snapshot.reports.len(), 1);
    assert_eq!(snapshot.tasks[0].state, adroit_model::TaskState::Succeeded);
    assert_eq!(
        snapshot.attempts[0].state,
        adroit_model::AttemptState::Succeeded
    );
    assert_eq!(snapshot.runs[0].state, adroit_model::RunState::Active);
    let mailbox = store.mailbox(owner, 0, 10).unwrap();
    assert_eq!(mailbox.messages.len(), 1);
    assert_eq!(mailbox.messages[0].message.id, report.message_id);
    assert_eq!(mailbox.messages[0].message.run_id, run);
    assert_eq!(mailbox.messages[0].delivery.state, DeliveryState::Stored);
}

#[test]
fn a_stale_attempt_cannot_complete_the_current_task() {
    let dir = tempfile::tempdir().unwrap();
    let mut store = Store::open(&dir.path().join("control/state.sqlite3")).unwrap();
    let (owner, run, attempt) = running_worker(&mut store, dir.path());
    store
        .put_task(
            RequestId::new(),
            owner,
            TaskPlan {
                id: attempt.task_id,
                run_id: run,
                expected_revision: Some(1),
                brief: TaskBrief {
                    instructions: "Review the changed inputs".into(),
                    ..Default::default()
                },
                dependencies: vec![],
            },
        )
        .unwrap();
    let record = store
        .report_task(
            RequestId::new(),
            SessionRef {
                id: attempt.session_id,
                generation: 1,
            },
            successful_report(&attempt),
        )
        .unwrap()
        .value;
    assert!(!record.applied);
    let snapshot = store.snapshot().unwrap();
    assert_eq!(snapshot.tasks[0].revision, 2);
    assert_ne!(snapshot.tasks[0].state, adroit_model::TaskState::Succeeded);
    assert_eq!(snapshot.reports.len(), 1);
}

#[test]
fn a_cancel_request_does_not_release_a_running_attempt_slot() {
    let dir = tempfile::tempdir().unwrap();
    let mut store = Store::open(&dir.path().join("control/state.sqlite3")).unwrap();
    let (owner, _, attempt) = running_worker(&mut store, dir.path());
    let wanted = TaskVersion {
        task_id: attempt.task_id,
        revision: 1,
    };
    store.cancel_task(RequestId::new(), owner, wanted).unwrap();
    assert_eq!(
        store.snapshot().unwrap().attempts[0].state,
        adroit_model::AttemptState::CancelRequested
    );
    assert!(
        store
            .reserve_attempt(RequestId::new(), owner, wanted, target())
            .is_err()
    );
    let mut report = successful_report(&attempt);
    report.result.status = adroit_model::ReportStatus::Cancelled;
    store
        .report_task(
            RequestId::new(),
            SessionRef {
                id: attempt.session_id,
                generation: 1,
            },
            report,
        )
        .unwrap();
    assert_eq!(
        store.snapshot().unwrap().attempts[0].state,
        adroit_model::AttemptState::Cancelled
    );
}

#[test]
fn another_worker_cannot_report_an_attempt_and_invalid_results_leave_no_notification() {
    let dir = tempfile::tempdir().unwrap();
    let mut store = Store::open(&dir.path().join("control/state.sqlite3")).unwrap();
    let (owner, _, attempt) = running_worker(&mut store, dir.path());
    let report = successful_report(&attempt);
    assert!(
        store
            .report_task(RequestId::new(), owner, report.clone())
            .is_err()
    );
    let mut invalid = report;
    invalid.result.summary.clear();
    assert!(
        store
            .report_task(
                RequestId::new(),
                SessionRef {
                    id: attempt.session_id,
                    generation: 1
                },
                invalid
            )
            .is_err()
    );
    let snapshot = store.snapshot().unwrap();
    assert!(snapshot.reports.is_empty());
    assert!(snapshot.messages.is_empty());
    assert_eq!(snapshot.tasks[0].state, adroit_model::TaskState::Running);
}

#[test]
fn revising_finished_work_requires_a_fresh_attempt() {
    let dir = tempfile::tempdir().unwrap();
    let mut store = Store::open(&dir.path().join("control/state.sqlite3")).unwrap();
    let (owner, run, attempt) = running_worker(&mut store, dir.path());
    store
        .report_task(
            RequestId::new(),
            SessionRef {
                id: attempt.session_id,
                generation: 1,
            },
            successful_report(&attempt),
        )
        .unwrap();
    let task = store
        .put_task(
            RequestId::new(),
            owner,
            TaskPlan {
                id: attempt.task_id,
                run_id: run,
                expected_revision: Some(1),
                brief: TaskBrief {
                    instructions: "Review the new candidate".into(),
                    ..Default::default()
                },
                dependencies: vec![],
            },
        )
        .unwrap()
        .value;
    assert_eq!(task.state, adroit_model::TaskState::Planned);
    assert_eq!(task.current_attempt, None);
    let version = TaskVersion {
        task_id: task.id,
        revision: 2,
    };
    store.queue_task(RequestId::new(), owner, version).unwrap();
    let fresh = store
        .reserve_attempt(RequestId::new(), owner, version, target())
        .unwrap()
        .value;
    assert_ne!(fresh.id, attempt.id);
    assert_ne!(fresh.session_id, attempt.session_id);
}

#[test]
fn a_late_success_cannot_override_an_accepted_cancellation() {
    let dir = tempfile::tempdir().unwrap();
    let mut store = Store::open(&dir.path().join("control/state.sqlite3")).unwrap();
    let (owner, _, attempt) = running_worker(&mut store, dir.path());
    let version = TaskVersion {
        task_id: attempt.task_id,
        revision: 1,
    };
    store.cancel_task(RequestId::new(), owner, version).unwrap();
    store.cancel_task(RequestId::new(), owner, version).unwrap();
    let report = store
        .report_task(
            RequestId::new(),
            SessionRef {
                id: attempt.session_id,
                generation: 1,
            },
            successful_report(&attempt),
        )
        .unwrap()
        .value;
    assert!(!report.applied);
    let snapshot = store.snapshot().unwrap();
    assert_eq!(snapshot.tasks[0].state, adroit_model::TaskState::Cancelled);
    assert_eq!(
        snapshot.attempts[0].state,
        adroit_model::AttemptState::Cancelled
    );
    assert_eq!(
        snapshot
            .messages
            .iter()
            .filter(|message| message.kind == MessageKind::Cancel)
            .count(),
        1
    );
}

#[test]
fn directory_change_results_need_evidence_from_the_assigned_working_area() {
    let dir = tempfile::tempdir().unwrap();
    let mut store = Store::open(&dir.path().join("control/state.sqlite3")).unwrap();
    let (_, _, attempt) = running_worker(&mut store, dir.path());
    let area_id = store.snapshot().unwrap().attempts[0].work_area_id.unwrap();
    let mut report = successful_report(&attempt);
    report.result.artifact = Some(adroit_model::TaskArtifact::Change {
        backend: adroit_model::WorkAreaBackend::Directory,
        work_area_id: area_id,
        revision: None,
        manifest: vec!["result.txt".into()],
        candidate_fingerprint: None,
    });
    let identity = SessionRef {
        id: attempt.session_id,
        generation: 1,
    };
    assert!(
        store
            .report_task(RequestId::new(), identity, report.clone())
            .is_err()
    );
    assert!(store.snapshot().unwrap().reports.is_empty());
    if let Some(adroit_model::TaskArtifact::Change {
        candidate_fingerprint,
        ..
    }) = &mut report.result.artifact
    {
        *candidate_fingerprint = Some("manifest-sha256:1234".into());
    }
    assert!(
        store
            .report_task(RequestId::new(), identity, report)
            .unwrap()
            .value
            .applied
    );
}

#[test]
fn an_uncertain_start_keeps_its_slot_until_the_terminal_is_reconciled() {
    let dir = tempfile::tempdir().unwrap();
    let mut store = Store::open(&dir.path().join("control/state.sqlite3")).unwrap();
    let (owner, run) = leader(&mut store, dir.path());
    let id = task(&mut store, owner, run, vec![]);
    let wanted = TaskVersion {
        task_id: id,
        revision: 1,
    };
    store.queue_task(RequestId::new(), owner, wanted).unwrap();
    let attempt = store
        .reserve_attempt(RequestId::new(), owner, wanted, target())
        .unwrap()
        .value;
    let identity = SessionRef {
        id: attempt.session_id,
        generation: 1,
    };
    store
        .start_uncertain(
            RequestId::new(),
            identity,
            "The terminal acknowledgement was lost".into(),
        )
        .unwrap();
    let snapshot = store.snapshot().unwrap();
    assert_eq!(
        snapshot.attempts[0].state,
        adroit_model::AttemptState::Uncertain
    );
    assert!(snapshot.attempts[0].state.holds_slot());
    assert_eq!(
        snapshot.sessions[1].state,
        adroit_model::ManagedSessionState::NeedsReconciliation
    );
    assert!(
        store
            .reserve_attempt(RequestId::new(), owner, wanted, target())
            .is_err()
    );
    assert_eq!(store.mailbox(owner, 0, 10).unwrap().messages.len(), 1);
}

fn running_dependent(
    store: &mut Store,
    cwd: &Path,
    owner: SessionRef,
    run: RunId,
    dependency: TaskId,
) -> adroit_model::AttemptRecord {
    let id = task(store, owner, run, vec![dependency]);
    let wanted = TaskVersion {
        task_id: id,
        revision: 1,
    };
    store.queue_task(RequestId::new(), owner, wanted).unwrap();
    let attempt = store
        .reserve_attempt(RequestId::new(), owner, wanted, target())
        .unwrap()
        .value;
    let area = store
        .register_work_area(RequestId::new(), owner, run, directory_area(cwd), true)
        .unwrap()
        .value;
    store
        .bind_work_area(RequestId::new(), owner, wanted, attempt.id, area.id)
        .unwrap();
    store
        .begin_start(RequestId::new(), owner, wanted, attempt.id)
        .unwrap();
    store
        .attach_session(
            RequestId::new(),
            SessionRef {
                id: attempt.session_id,
                generation: 1,
            },
            cwd.canonicalize().unwrap(),
            "000000000000000000000000000000cd".into(),
            "dependent-conversation".into(),
        )
        .unwrap();
    attempt
}

#[test]
fn revising_an_input_invalidates_completed_dependents_but_leaves_unrelated_work_queued() {
    let dir = tempfile::tempdir().unwrap();
    let mut store = Store::open(&dir.path().join("control/state.sqlite3")).unwrap();
    let (owner, run, producer) = running_worker(&mut store, dir.path());
    store
        .report_task(
            RequestId::new(),
            SessionRef {
                id: producer.session_id,
                generation: 1,
            },
            successful_report(&producer),
        )
        .unwrap();
    let dependent = running_dependent(&mut store, dir.path(), owner, run, producer.task_id);
    assert_eq!(
        dependent.input_versions,
        vec![TaskVersion {
            task_id: producer.task_id,
            revision: 1
        }]
    );
    store
        .report_task(
            RequestId::new(),
            SessionRef {
                id: dependent.session_id,
                generation: 1,
            },
            successful_report(&dependent),
        )
        .unwrap();
    let unrelated = task(&mut store, owner, run, vec![]);
    store
        .queue_task(
            RequestId::new(),
            owner,
            TaskVersion {
                task_id: unrelated,
                revision: 1,
            },
        )
        .unwrap();
    store
        .put_task(
            RequestId::new(),
            owner,
            TaskPlan {
                id: producer.task_id,
                run_id: run,
                expected_revision: Some(1),
                brief: TaskBrief {
                    instructions: "Use the corrected inputs".into(),
                    ..Default::default()
                },
                dependencies: vec![],
            },
        )
        .unwrap();
    let snapshot = store.snapshot().unwrap();
    let changed = snapshot
        .tasks
        .iter()
        .find(|task| task.id == dependent.task_id)
        .unwrap();
    assert_eq!(changed.revision, 2);
    assert_eq!(changed.state, adroit_model::TaskState::Planned);
    assert_eq!(changed.current_attempt, None);
    let unchanged = snapshot
        .tasks
        .iter()
        .find(|task| task.id == unrelated)
        .unwrap();
    assert_eq!(unchanged.revision, 1);
    assert_eq!(unchanged.state, adroit_model::TaskState::Queued);
}

#[test]
fn an_input_change_stops_a_running_dependent_and_its_late_report_stays_historical() {
    let dir = tempfile::tempdir().unwrap();
    let mut store = Store::open(&dir.path().join("control/state.sqlite3")).unwrap();
    let (owner, run, producer) = running_worker(&mut store, dir.path());
    store
        .report_task(
            RequestId::new(),
            SessionRef {
                id: producer.session_id,
                generation: 1,
            },
            successful_report(&producer),
        )
        .unwrap();
    let dependent = running_dependent(&mut store, dir.path(), owner, run, producer.task_id);
    let grandchild = task(&mut store, owner, run, vec![dependent.task_id]);
    store
        .put_task(
            RequestId::new(),
            owner,
            TaskPlan {
                id: producer.task_id,
                run_id: run,
                expected_revision: Some(1),
                brief: TaskBrief {
                    instructions: "Use the corrected inputs".into(),
                    ..Default::default()
                },
                dependencies: vec![],
            },
        )
        .unwrap();
    let snapshot = store.snapshot().unwrap();
    assert_eq!(
        snapshot
            .attempts
            .iter()
            .find(|attempt| attempt.id == dependent.id)
            .unwrap()
            .state,
        adroit_model::AttemptState::CancelRequested
    );
    let result = store
        .report_task(
            RequestId::new(),
            SessionRef {
                id: dependent.session_id,
                generation: 1,
            },
            successful_report(&dependent),
        )
        .unwrap();
    assert!(!result.value.applied);
    let snapshot = store.snapshot().unwrap();
    let changed = snapshot
        .tasks
        .iter()
        .find(|task| task.id == dependent.task_id)
        .unwrap();
    assert_eq!(changed.revision, 2);
    assert_eq!(changed.state, adroit_model::TaskState::Planned);
    assert_eq!(changed.current_attempt, None);
    assert_eq!(
        snapshot
            .tasks
            .iter()
            .find(|task| task.id == grandchild)
            .unwrap()
            .revision,
        2
    );
}

#[test]
fn stopping_preparation_releases_a_cancelled_attempt_without_failing_the_new_task_revision() {
    let dir = tempfile::tempdir().unwrap();
    let mut store = Store::open(&dir.path().join("control/state.sqlite3")).unwrap();
    let (owner, run) = leader(&mut store, dir.path());
    let id = task(&mut store, owner, run, vec![]);
    let wanted = TaskVersion {
        task_id: id,
        revision: 1,
    };
    store.queue_task(RequestId::new(), owner, wanted).unwrap();
    let attempt = store
        .reserve_attempt(RequestId::new(), owner, wanted, target())
        .unwrap()
        .value;
    store
        .put_task(
            RequestId::new(),
            owner,
            TaskPlan {
                id,
                run_id: run,
                expected_revision: Some(1),
                brief: TaskBrief {
                    instructions: "Use the corrected inputs".into(),
                    ..Default::default()
                },
                dependencies: vec![],
            },
        )
        .unwrap();
    let result = store.fail_start(
        RequestId::new(),
        SessionRef {
            id: attempt.session_id,
            generation: 1,
        },
        "Preparation stopped before the program started".into(),
    );
    assert!(result.is_ok(), "{result:?}");
    let snapshot = store.snapshot().unwrap();
    assert_eq!(
        snapshot.attempts[0].state,
        adroit_model::AttemptState::Cancelled
    );
    assert_eq!(snapshot.tasks[0].revision, 2);
    assert_eq!(snapshot.tasks[0].state, adroit_model::TaskState::Planned);
    assert_eq!(snapshot.tasks[0].current_attempt, None);
}

#[test]
fn an_attempt_without_recorded_input_versions_cannot_succeed_using_unknown_inputs() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("control/state.sqlite3");
    let mut store = Store::open(&path).unwrap();
    let (owner, run, producer) = running_worker(&mut store, dir.path());
    store
        .report_task(
            RequestId::new(),
            SessionRef {
                id: producer.session_id,
                generation: 1,
            },
            successful_report(&producer),
        )
        .unwrap();
    let dependent = running_dependent(&mut store, dir.path(), owner, run, producer.task_id);
    drop(store);
    let connection = rusqlite::Connection::open(&path).unwrap();
    connection
        .execute(
            "UPDATE attempts SET data = json_remove(data,'$.input_versions') WHERE id = ?1",
            [dependent.id.to_string()],
        )
        .unwrap();
    drop(connection);
    let mut store = Store::open(&path).unwrap();
    let result = store
        .report_task(
            RequestId::new(),
            SessionRef {
                id: dependent.session_id,
                generation: 1,
            },
            successful_report(&dependent),
        )
        .unwrap();
    assert!(!result.value.applied);
    let snapshot = store.snapshot().unwrap();
    let task = snapshot
        .tasks
        .iter()
        .find(|task| task.id == dependent.task_id)
        .unwrap();
    assert_eq!(task.state, adroit_model::TaskState::Planned);
    assert_eq!(task.revision, 2);
    assert_eq!(task.current_attempt, None);
}

#[test]
fn concurrent_live_source_registration_keeps_one_ready_identity_across_runs() {
    use adroit_model::{WorkAreaBackend, WorkAreaId, WorkAreaState};
    for backend in [
        WorkAreaBackend::Jj,
        WorkAreaBackend::Git,
        WorkAreaBackend::Directory,
    ] {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("control/state.sqlite3");
        let mut store = Store::open(&path).unwrap();
        let (first, run_a) = leader(&mut store, dir.path());
        let (second, run_b) = leader(&mut store, dir.path());
        let mut area = directory_area(dir.path());
        area.backend = backend;
        area.repository =
            (backend != WorkAreaBackend::Directory).then(|| dir.path().join("repository"));
        area.state = WorkAreaState::Ready;
        let barrier = std::sync::Arc::new(std::sync::Barrier::new(2));
        let jobs: Vec<_> = [(first, run_a), (second, run_b)]
            .into_iter()
            .map(|(owner, run)| {
                let path = path.clone();
                let mut area = area.clone();
                area.id = WorkAreaId::new();
                let barrier = barrier.clone();
                std::thread::spawn(move || {
                    let mut store = Store::open(&path).unwrap();
                    barrier.wait();
                    store
                        .register_source_work_area(RequestId::new(), owner, run, area)
                        .unwrap()
                        .value
                })
            })
            .collect();
        let areas: Vec<_> = jobs.into_iter().map(|job| job.join().unwrap()).collect();
        assert_eq!(areas[0], areas[1]);
        let snapshot = store.snapshot().unwrap();
        assert_eq!(snapshot.work_areas.len(), 1);
        assert_eq!(snapshot.run_work_areas.len(), 2);
        assert!(snapshot.run_work_areas.iter().all(|a| !a.is_default));
        // Re-identification must retain all recorded metadata, including a native base.
        let mut identified = area;
        identified.id = WorkAreaId::new();
        identified.description = "Another task".into();
        let kept = store
            .register_source_work_area(RequestId::new(), first, run_a, identified)
            .unwrap()
            .value;
        assert_eq!(kept, areas[0]);
    }
}

#[test]
fn live_source_registration_reuses_ready_native_metadata_but_rejects_unavailable_or_changed_identity()
 {
    use adroit_model::{WorkAreaBackend, WorkAreaId, WorkAreaState};
    for state in [
        WorkAreaState::Ready,
        WorkAreaState::Preparing,
        WorkAreaState::Failed,
    ] {
        let dir = tempfile::tempdir().unwrap();
        let mut store = Store::open(&dir.path().join("control/state.sqlite3")).unwrap();
        let (owner, run_a) = leader(&mut store, dir.path());
        let (other, run_b) = leader(&mut store, dir.path());
        let mut area = directory_area(dir.path());
        area.backend = WorkAreaBackend::Jj;
        area.repository = Some(dir.path().join("repository"));
        area.base_revision = Some("a".repeat(40));
        area.native_name = Some("original".into());
        if state == WorkAreaState::Ready {
            area.state = state;
        }
        store
            .register_work_area(RequestId::new(), owner, run_a, area.clone(), true)
            .unwrap();
        if state == WorkAreaState::Failed {
            area = store
                .finish_work_area(RequestId::new(), area.id, Err("failed".into()))
                .unwrap()
                .value;
        }
        let mut identified = area.clone();
        identified.id = WorkAreaId::new();
        identified.state = WorkAreaState::Ready;
        identified.base_revision = None;
        identified.native_name = None;
        let result =
            store.register_source_work_area(RequestId::new(), other, run_b, identified.clone());
        if state == WorkAreaState::Ready {
            assert_eq!(result.unwrap().value, area);
            assert_eq!(store.snapshot().unwrap().run_work_areas.len(), 2);
        } else {
            assert!(result.is_err());
            assert_eq!(store.snapshot().unwrap().run_work_areas.len(), 1);
        }
        identified.repository = Some(dir.path().join("different-repository"));
        assert!(
            store
                .register_source_work_area(RequestId::new(), other, run_b, identified)
                .is_err()
        );
        assert_eq!(store.snapshot().unwrap().work_areas, vec![area]);
    }
}

fn spoil_task(path: &Path, task: TaskId) {
    rusqlite::Connection::open(path)
        .unwrap()
        .execute(
            "UPDATE tasks SET data = json_set(data, '$.unknown_field', 1) WHERE id = ?1",
            [task.to_string()],
        )
        .unwrap();
}

#[test]
fn an_unreadable_record_holds_only_the_run_it_belongs_to() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("control/state.sqlite3");
    let mut store = Store::open(&path).unwrap();
    let (spoiled_leader, spoiled) = leader(&mut store, dir.path());
    let spoiled_task = task(&mut store, spoiled_leader, spoiled, vec![]);
    let (kept_leader, kept) = leader(&mut store, dir.path());
    let kept_task = task(&mut store, kept_leader, kept, vec![]);
    spoil_task(&path, spoiled_task);

    let readable = store.readable_snapshot().unwrap();

    let snapshot = readable.snapshot;
    assert_eq!(
        snapshot.runs.iter().map(|run| run.id).collect::<Vec<_>>(),
        vec![kept]
    );
    assert_eq!(
        snapshot
            .tasks
            .iter()
            .map(|task| task.id)
            .collect::<Vec<_>>(),
        vec![kept_task]
    );
    assert!(
        snapshot
            .sessions
            .iter()
            .all(|session| session.binding.logical_id() != spoiled_leader.id)
    );
    assert_eq!(readable.unreadable.len(), 1);
    let held = &readable.unreadable[0];
    assert_eq!(held.run_id, spoiled);
    assert_eq!(
        held.error.code,
        adroit::control::store::ErrorCode::CorruptStore
    );
    assert!(
        held.error.message.contains("unknown_field"),
        "{}",
        held.error
    );
}

#[test]
fn a_whole_snapshot_names_the_run_whose_record_cannot_be_read() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("control/state.sqlite3");
    let mut store = Store::open(&path).unwrap();
    let (owner, run) = leader(&mut store, dir.path());
    let spoiled = task(&mut store, owner, run, vec![]);
    spoil_task(&path, spoiled);

    let error = store.snapshot().unwrap_err();

    assert_eq!(error.code, adroit::control::store::ErrorCode::CorruptStore);
    assert!(error.message.contains(&run.to_string()), "{error}");
}
