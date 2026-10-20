use adroit::control::store::Store;
use adroit_model::{
    Agent, AgentMessage, AgentPermissions, DeliveryState, LogicalSessionId, MessageId, MessageKind,
    ModelSource, RequestId, RunBrief, RunId, SessionRef, TargetSelection, TaskBrief, TaskId,
    TaskPlan, TaskVersion,
};
use std::path::Path;

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
fn transport_acceptance_is_distinct_from_agent_acknowledgment() {
    let dir = tempfile::tempdir().unwrap();
    let mut store = Store::open(&dir.path().join("control/state.sqlite3")).unwrap();
    let (owner, message) = pending_report(&mut store, dir.path());
    let delivery = store
        .accept_delivery(RequestId::new(), message.id, owner.id)
        .unwrap()
        .value;
    assert_eq!(delivery.state, DeliveryState::Accepted);
    let acknowledgment = store
        .acknowledge_message(RequestId::new(), owner, message.id)
        .unwrap()
        .value;
    assert_eq!(acknowledgment.state, DeliveryState::Acknowledged);
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
        .accept_delivery(RequestId::new(), message.id, owner.id)
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
    store
        .attach_session(
            RequestId::new(),
            identity,
            dir.path().into(),
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
        .accept_delivery(RequestId::new(), message.id, owner.id)
        .unwrap();
    let delivered = &store.snapshot().unwrap().deliveries[0];
    assert_eq!(delivered.state, DeliveryState::Accepted);
    assert_eq!(delivered.attempts, 1);
}
