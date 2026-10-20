use adroit::control::store::Store;
use adroit_model::*;
use std::path::Path;

fn running_worker(store: &mut Store, path: &Path) -> (SessionRef, RunId, AttemptRecord, String) {
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
                title: "Request".into(),
                goal: "Finish the work".into(),
                ..Default::default()
            },
        )
        .unwrap();
    let task = store
        .put_task(
            RequestId::new(),
            leader,
            TaskPlan {
                id: TaskId::new(),
                run_id: run,
                expected_revision: None,
                brief: TaskBrief {
                    instructions: "Investigate".into(),
                    ..Default::default()
                },
                dependencies: vec![],
            },
        )
        .unwrap()
        .value;
    let wanted = TaskVersion {
        task_id: task.id,
        revision: task.revision,
    };
    store.queue_task(RequestId::new(), leader, wanted).unwrap();
    let attempt = store
        .reserve_attempt(RequestId::new(), leader, wanted, TargetSelection::default())
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
                description: "Working directory".into(),
                state: WorkAreaState::Ready,
                error: None,
            },
            true,
        )
        .unwrap()
        .value;
    store
        .bind_work_area(RequestId::new(), leader, wanted, attempt.id, area.id)
        .unwrap();
    store
        .begin_start(RequestId::new(), leader, wanted, attempt.id)
        .unwrap();
    let terminal = attempt.id.to_string().replace('-', "");
    store
        .attach_session(
            RequestId::new(),
            SessionRef {
                id: attempt.session_id,
                generation: 1,
            },
            path,
            terminal.clone(),
            MessageId::new().to_string(),
        )
        .unwrap();
    (leader, run, attempt, terminal)
}

#[test]
fn a_confirmed_native_exit_releases_the_worker_slot_without_inventing_success_and_notifies_once() {
    let dir = tempfile::tempdir().unwrap();
    let mut store = Store::open(&dir.path().join("control/state.sqlite3")).unwrap();
    let (leader, _, attempt, terminal) = running_worker(&mut store, dir.path());
    let worker = SessionRef {
        id: attempt.session_id,
        generation: 1,
    };
    store
        .observe_terminal_exit(RequestId::new(), worker, terminal.clone())
        .unwrap();
    store
        .observe_terminal_exit(RequestId::new(), worker, terminal)
        .unwrap();
    let snapshot = store.snapshot().unwrap();
    assert_eq!(snapshot.attempts[0].state, AttemptState::Failed);
    assert_eq!(snapshot.tasks[0].state, TaskState::Failed);
    assert_eq!(
        snapshot
            .sessions
            .iter()
            .find(|session| session.reference() == worker)
            .unwrap()
            .state,
        ManagedSessionState::Exited
    );
    assert!(snapshot.completions.is_empty());
    assert_eq!(
        snapshot
            .messages
            .iter()
            .filter(|message| message.to == leader.id)
            .count(),
        1
    );
}

#[test]
fn a_missing_terminal_keeps_its_slot_until_a_confirmed_exit_and_does_not_duplicate_questions() {
    let dir = tempfile::tempdir().unwrap();
    let mut store = Store::open(&dir.path().join("control/state.sqlite3")).unwrap();
    let (_, _, attempt, terminal) = running_worker(&mut store, dir.path());
    let worker = SessionRef {
        id: attempt.session_id,
        generation: 1,
    };
    for _ in 0..2 {
        store
            .observe_terminal_missing(RequestId::new(), worker, terminal.clone())
            .unwrap();
    }
    let snapshot = store.snapshot().unwrap();
    assert_eq!(snapshot.attempts[0].state, AttemptState::Uncertain);
    assert!(snapshot.attempts[0].state.holds_slot());
    assert_eq!(
        snapshot
            .messages
            .iter()
            .filter(|message| message.blocks_work)
            .count(),
        1
    );
    store
        .observe_terminal_exit(RequestId::new(), worker, terminal)
        .unwrap();
    assert_eq!(
        store.snapshot().unwrap().attempts[0].state,
        AttemptState::Failed
    );
}

#[test]
fn an_exit_confirms_cancellation_and_cannot_overwrite_a_newer_task_revision() {
    let dir = tempfile::tempdir().unwrap();
    let mut store = Store::open(&dir.path().join("control/state.sqlite3")).unwrap();
    let (leader, run, attempt, terminal) = running_worker(&mut store, dir.path());
    store
        .put_task(
            RequestId::new(),
            leader,
            TaskPlan {
                id: attempt.task_id,
                run_id: run,
                expected_revision: Some(1),
                brief: TaskBrief {
                    instructions: "Revised investigation".into(),
                    ..Default::default()
                },
                dependencies: vec![],
            },
        )
        .unwrap();
    store
        .observe_terminal_exit(
            RequestId::new(),
            SessionRef {
                id: attempt.session_id,
                generation: 1,
            },
            terminal,
        )
        .unwrap();
    let snapshot = store.snapshot().unwrap();
    assert_eq!(snapshot.attempts[0].state, AttemptState::Cancelled);
    assert_eq!(snapshot.tasks[0].revision, 2);
    assert_eq!(snapshot.tasks[0].state, TaskState::Planned);
    assert!(snapshot.tasks[0].current_attempt.is_none());
}

#[test]
fn a_foreign_terminal_cannot_supply_exit_evidence_for_this_worker() {
    let dir = tempfile::tempdir().unwrap();
    let mut store = Store::open(&dir.path().join("control/state.sqlite3")).unwrap();
    let (_, _, attempt, _) = running_worker(&mut store, dir.path());
    assert!(
        store
            .observe_terminal_exit(
                RequestId::new(),
                SessionRef {
                    id: attempt.session_id,
                    generation: 1
                },
                "another-terminal".into()
            )
            .is_err()
    );
    assert_eq!(
        store.snapshot().unwrap().attempts[0].state,
        AttemptState::Running
    );
}

fn success(attempt: &AttemptRecord) -> TaskReport {
    TaskReport {
        message_id: MessageId::new(),
        task: TaskVersion {
            task_id: attempt.task_id,
            revision: attempt.task_revision,
        },
        attempt_id: attempt.id,
        result: TaskResult {
            status: ReportStatus::Succeeded,
            summary: "Investigation finished".into(),
            artifact: Some(TaskArtifact::Report {
                data: serde_json::json!({"result":"done"}),
                files: vec![],
                reviewed_revision: None,
            }),
            checks: vec![],
            unresolved: vec![],
        },
    }
}

#[test]
fn a_missing_terminal_after_a_successful_report_does_not_invalidate_the_result_or_block_completion()
{
    let dir = tempfile::tempdir().unwrap();
    let mut store = Store::open(&dir.path().join("control/state.sqlite3")).unwrap();
    let (owner, run, attempt, terminal) = running_worker(&mut store, dir.path());
    let worker = SessionRef {
        id: attempt.session_id,
        generation: 1,
    };
    store
        .report_task(RequestId::new(), worker, success(&attempt))
        .unwrap();
    store
        .observe_terminal_missing(RequestId::new(), worker, terminal)
        .unwrap();
    let snapshot = store.snapshot().unwrap();
    assert_eq!(snapshot.tasks[0].state, TaskState::Succeeded);
    assert_eq!(snapshot.attempts[0].state, AttemptState::Succeeded);
    store
        .complete_run(
            RequestId::new(),
            owner,
            run,
            snapshot.runs[0].plan_revision,
            "Done".into(),
        )
        .unwrap();
}

#[tokio::test]
async fn an_unreconciled_agent_can_read_its_assignment_but_cannot_apply_results_or_claim_delivery()
{
    use adroit::control::{client::Client, protocol::Command, service::Service};
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().join("control");
    let mut store = Store::open(&root.join("state.sqlite3")).unwrap();
    let (_, _, attempt, terminal) = running_worker(&mut store, dir.path());
    let worker = SessionRef {
        id: attempt.session_id,
        generation: 1,
    };
    let client = Client::session(&root, store.grant(worker).unwrap());
    store
        .observe_terminal_missing(RequestId::new(), worker, terminal.clone())
        .unwrap();
    drop(store);
    let service = Service::bind(&root).await.unwrap();
    client
        .request(RequestId::new(), Command::Context)
        .await
        .unwrap();
    assert!(
        client
            .request(
                RequestId::new(),
                Command::ReportTask {
                    report: success(&attempt)
                }
            )
            .await
            .is_err()
    );
    assert!(
        client
            .request(
                RequestId::new(),
                Command::ClaimDelivery {
                    message_id: MessageId::new()
                }
            )
            .await
            .is_err()
    );
    assert!(
        client
            .request(
                RequestId::new(),
                Command::ConfirmTerminalExit {
                    identity: worker,
                    terminal_id: terminal.clone()
                }
            )
            .await
            .is_err()
    );
    Client::human(&root)
        .unwrap()
        .request(
            RequestId::new(),
            Command::ConfirmTerminalExit {
                identity: worker,
                terminal_id: terminal,
            },
        )
        .await
        .unwrap();
    assert_eq!(
        service
            .actor()
            .call(|store| store.snapshot())
            .await
            .unwrap()
            .attempts[0]
            .state,
        AttemptState::Failed
    );
}
