#[path = "support/daemon.rs"]
mod native_daemon;
use adroit::control::store::Store;
use adroit_model::*;
use std::path::Path;

fn running_worker(store: &mut Store, path: &Path) -> (SessionRef, RunId, AttemptRecord, String) {
    running_worker_at(store, path, None)
}

fn running_worker_at(
    store: &mut Store,
    path: &Path,
    terminal: Option<String>,
) -> (SessionRef, RunId, AttemptRecord, String) {
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
    let terminal = terminal.unwrap_or_else(|| attempt.id.to_string().replace('-', ""));
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
fn manually_closing_a_leader_cancels_its_run_and_cannot_restart_or_be_revived() {
    let dir = tempfile::tempdir().unwrap();
    let database = dir.path().join("control/state.sqlite3");
    let mut store = Store::open(&database).unwrap();
    let (leader, run, attempt, _) = running_worker(&mut store, dir.path());
    let terminal = "a".repeat(32);
    store
        .attach_session(
            RequestId::new(),
            leader,
            dir.path().canonicalize().unwrap(),
            terminal.clone(),
            MessageId::new().to_string(),
        )
        .unwrap();
    store
        .close_session(RequestId::new(), leader, terminal.clone())
        .unwrap();
    store
        .close_session(RequestId::new(), leader, terminal.clone())
        .unwrap();
    store
        .observe_terminal_exit(RequestId::new(), leader, terminal)
        .unwrap();
    store
        .start_uncertain(RequestId::new(), leader, "Late startup response".into())
        .unwrap();
    assert!(store.restart_leader(RequestId::new(), leader).is_err());
    assert!(store.require_available_session(leader).is_err());
    drop(store);
    let mut store = Store::open(&database).unwrap();
    let snapshot = store.snapshot().unwrap();
    assert_eq!(
        snapshot
            .runs
            .iter()
            .find(|record| record.id == run)
            .unwrap()
            .state,
        RunState::Cancelling
    );
    assert_eq!(
        snapshot
            .attempts
            .iter()
            .find(|record| record.id == attempt.id)
            .unwrap()
            .state,
        AttemptState::CancelRequested
    );
    let session = snapshot
        .sessions
        .iter()
        .find(|session| session.reference() == leader)
        .unwrap();
    assert_eq!(serde_json::to_value(session.state).unwrap(), "closed");
}

#[test]
fn closing_a_different_terminal_cannot_cancel_the_leaders_run() {
    let dir = tempfile::tempdir().unwrap();
    let mut store = Store::open(&dir.path().join("control/state.sqlite3")).unwrap();
    let (leader, _, _, _) = running_worker(&mut store, dir.path());
    store
        .attach_session(
            RequestId::new(),
            leader,
            dir.path().canonicalize().unwrap(),
            "a".repeat(32),
            MessageId::new().to_string(),
        )
        .unwrap();
    assert!(
        store
            .close_session(RequestId::new(), leader, "b".repeat(32))
            .is_err()
    );
    assert_eq!(store.snapshot().unwrap().runs[0].state, RunState::Active);
}

#[tokio::test]
async fn cleanup_closes_reported_and_orphaned_workers_but_keeps_unfinished_work_and_other_sessions()
{
    use adroit::control::{actor::Actor, observer::Observer};
    use adroit_term::{Daemon, Spawn, tags};
    use std::os::unix::fs::PermissionsExt;
    for outcome in [
        None,
        Some(ReportStatus::Succeeded),
        Some(ReportStatus::Failed),
        Some(ReportStatus::Blocked),
        Some(ReportStatus::Cancelled),
    ] {
        let dir = tempfile::Builder::new()
            .prefix("adroit-close-")
            .tempdir_in("/tmp")
            .unwrap();
        std::fs::set_permissions(dir.path(), std::fs::Permissions::from_mode(0o700)).unwrap();
        let daemon = Daemon::new(dir.path().join("d.sock"));
        let native = native_daemon::serve_process(daemon.socket().into());
        for _ in 0..100 {
            if daemon.sessions().await.is_ok() {
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        }
        let spawn = || Spawn {
            program: "/bin/sleep".into(),
            args: vec!["60".into()],
            cwd: dir.path().into(),
            ..Default::default()
        };
        let terminal = daemon.spawn(spawn()).await.unwrap();
        let other = daemon.spawn(spawn()).await.unwrap();
        let database = dir.path().join("control/state.sqlite3");
        let mut store = Store::open(&database).unwrap();
        let (leader, _, attempt, terminal_id) =
            running_worker_at(&mut store, dir.path(), Some(format!("{terminal:032x}")));
        let worker = store
            .snapshot()
            .unwrap()
            .sessions
            .into_iter()
            .find(|session| session.binding.work().is_some())
            .unwrap();
        let mut ownership = tags::tagged(
            "adroit.binding",
            &serde_json::to_string(&worker.binding).unwrap(),
        );
        ownership.extend(tags::tagged("adroit.generation", "1"));
        daemon.tag(terminal, ownership, vec![]).await.unwrap();
        if let Some(status) = outcome {
            let mut report = success(&attempt);
            report.result.status = status;
            store
                .report_task(RequestId::new(), worker.reference(), report)
                .unwrap();
        }
        drop(store);
        let actor = Actor::open(database).await.unwrap();
        let mut observer = Observer::default();
        if outcome.is_none() {
            observer.tick(&actor, &daemon).await.unwrap();
            assert!(
                daemon
                    .sessions()
                    .await
                    .unwrap()
                    .iter()
                    .any(|session| session.id == terminal)
            );
            let cwd = dir.path().canonicalize().unwrap();
            actor
                .call(move |store| {
                    store.attach_session(
                        RequestId::new(),
                        leader,
                        cwd,
                        "a".repeat(32),
                        MessageId::new().to_string(),
                    )?;
                    store
                        .close_session(RequestId::new(), leader, "a".repeat(32))
                        .map(|_| ())
                })
                .await
                .unwrap();
        }
        observer.tick(&actor, &daemon).await.unwrap();
        observer.tick(&actor, &daemon).await.unwrap();
        let snapshot = actor.call(|store| store.snapshot()).await.unwrap();
        let worker = snapshot
            .sessions
            .iter()
            .find(|session| session.terminal_id.as_ref() == Some(&terminal_id))
            .unwrap();
        assert_eq!(worker.state, ManagedSessionState::Exited);
        assert_eq!(snapshot.reports.len(), usize::from(outcome.is_some()));
        if outcome.is_none() {
            assert_eq!(snapshot.runs[0].state, RunState::Cancelled);
        }
        let remaining = daemon.sessions().await.unwrap();
        assert!(remaining.iter().all(|session| session.id != terminal));
        assert!(remaining.iter().any(|session| session.id == other));
        daemon.end(other).await.unwrap();
        native.abort();
    }
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

#[test]
fn a_reconciled_and_restarted_leader_can_complete_without_answering_its_own_notification() {
    use adroit::control::runtime::RuntimeLaunch;
    use adroit::settings::Settings;

    let dir = tempfile::tempdir().unwrap();
    let mut store = Store::open(&dir.path().join("control/state.sqlite3")).unwrap();
    let (owner, run, attempt, _) = running_worker(&mut store, dir.path());
    store
        .report_task(
            RequestId::new(),
            SessionRef {
                id: attempt.session_id,
                generation: 1,
            },
            success(&attempt),
        )
        .unwrap();
    store
        .remember_launch(
            RequestId::new(),
            owner,
            RuntimeLaunch::new(Settings::default()),
        )
        .unwrap();
    let terminal = MessageId::new().to_string().replace('-', "");
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
        .observe_terminal_missing(RequestId::new(), owner, terminal.clone())
        .unwrap();
    store
        .observe_terminal_exit(RequestId::new(), owner, terminal)
        .unwrap();
    let restarted = store
        .restart_leader(RequestId::new(), owner)
        .unwrap()
        .value
        .reference();
    store
        .attach_session(
            RequestId::new(),
            restarted,
            dir.path().into(),
            MessageId::new().to_string().replace('-', ""),
            MessageId::new().to_string(),
        )
        .unwrap();
    let revision = store.snapshot().unwrap().runs[0].plan_revision;
    store
        .complete_run(RequestId::new(), restarted, run, revision, "Done".into())
        .unwrap();
    let snapshot = store.snapshot().unwrap();
    let notifications: Vec<_> = snapshot
        .messages
        .iter()
        .filter(|message| message.from == owner.id && message.to == owner.id)
        .collect();
    assert!(!notifications.is_empty());
    assert!(
        notifications
            .iter()
            .all(|message| message.kind == MessageKind::Note && !message.blocks_work)
    );
    assert_eq!(snapshot.runs[0].state, RunState::Succeeded);
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

fn pending_worker(store: &mut Store, path: &Path, starting: bool) -> (SessionRef, AttemptRecord) {
    let (owner, run, _, _) = running_worker(store, path);
    let task = store
        .put_task(
            RequestId::new(),
            owner,
            TaskPlan {
                id: TaskId::new(),
                run_id: run,
                expected_revision: None,
                brief: TaskBrief {
                    instructions: "Pending work".into(),
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
    store.queue_task(RequestId::new(), owner, wanted).unwrap();
    let attempt = store
        .reserve_attempt(RequestId::new(), owner, wanted, TargetSelection::default())
        .unwrap()
        .value;
    let area = store.snapshot().unwrap().work_areas[0].id;
    if starting {
        store
            .bind_work_area(RequestId::new(), owner, wanted, attempt.id, area)
            .unwrap();
        store
            .begin_start(RequestId::new(), owner, wanted, attempt.id)
            .unwrap();
    }
    (owner, attempt)
}

#[test]
fn interrupted_preparation_fails_without_respawning_but_a_start_intent_remains_uncertain() {
    for starting in [false, true] {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("control/state.sqlite3");
        let mut store = Store::open(&path).unwrap();
        let (_, attempt) = pending_worker(&mut store, dir.path(), starting);
        let identity = SessionRef {
            id: attempt.session_id,
            generation: 1,
        };
        assert!(store.interrupted_startups().unwrap().contains(&identity));
        drop(store);
        let mut store = Store::open(&path).unwrap();
        store.recover_startup(RequestId::new(), identity).unwrap();
        let snapshot = store.snapshot().unwrap();
        let attempt = snapshot
            .attempts
            .iter()
            .find(|record| record.id == attempt.id)
            .unwrap();
        assert_eq!(
            attempt.state,
            if starting {
                AttemptState::Uncertain
            } else {
                AttemptState::Failed
            }
        );
        assert_eq!(attempt.state.holds_slot(), starting);
        assert_eq!(snapshot.attempts.len(), 2);
        let count = snapshot.messages.len();
        store.recover_startup(RequestId::new(), identity).unwrap();
        assert_eq!(store.snapshot().unwrap().messages.len(), count);
    }
}

#[test]
fn a_session_attached_after_recovery_was_listed_is_not_failed_by_the_old_preparation_record() {
    let dir = tempfile::tempdir().unwrap();
    let mut store = Store::open(&dir.path().join("control/state.sqlite3")).unwrap();
    let leader = store
        .register_leader(
            RequestId::new(),
            LogicalSessionId::new(),
            Agent::Codex,
            dir.path().into(),
        )
        .unwrap()
        .value;
    assert!(
        store
            .interrupted_startups()
            .unwrap()
            .contains(&leader.reference())
    );
    store
        .attach_session(
            RequestId::new(),
            leader.reference(),
            dir.path().into(),
            MessageId::new().to_string().replace('-', ""),
            MessageId::new().to_string(),
        )
        .unwrap();
    store
        .recover_startup(RequestId::new(), leader.reference())
        .unwrap();
    assert_eq!(
        store.snapshot().unwrap().sessions[0].state,
        ManagedSessionState::Running
    );
}

#[test]
fn interrupted_native_working_area_preparation_is_retained_for_reconciliation() {
    let dir = tempfile::tempdir().unwrap();
    let mut store = Store::open(&dir.path().join("control/state.sqlite3")).unwrap();
    let (owner, attempt) = pending_worker(&mut store, dir.path(), false);
    let run = store.snapshot().unwrap().runs[0].id;
    let source = dir.path().canonicalize().unwrap();
    let area = store
        .register_work_area(
            RequestId::new(),
            owner,
            run,
            WorkArea {
                id: WorkAreaId::new(),
                backend: WorkAreaBackend::Jj,
                path: source.join("partial-native-area"),
                source: source.clone(),
                source_root: source.clone(),
                repository: Some(source),
                base_revision: Some("a".repeat(40)),
                native_name: Some("pending".into()),
                description: "Pending native workspace".into(),
                state: WorkAreaState::Preparing,
                error: None,
            },
            false,
        )
        .unwrap()
        .value;
    store
        .bind_work_area(
            RequestId::new(),
            owner,
            TaskVersion {
                task_id: attempt.task_id,
                revision: 1,
            },
            attempt.id,
            area.id,
        )
        .unwrap();
    store
        .recover_startup(
            RequestId::new(),
            SessionRef {
                id: attempt.session_id,
                generation: 1,
            },
        )
        .unwrap();
    assert_eq!(
        store.work_area(area.id).unwrap().state,
        WorkAreaState::NeedsReconciliation
    );
    assert!(
        store
            .finish_work_area(RequestId::new(), area.id, Ok(()))
            .is_err()
    );
}

#[test]
fn cancelled_preparation_releases_its_slot_only_when_no_start_intent_was_recorded() {
    for starting in [false, true] {
        let dir = tempfile::tempdir().unwrap();
        let mut store = Store::open(&dir.path().join("control/state.sqlite3")).unwrap();
        let (owner, attempt) = pending_worker(&mut store, dir.path(), starting);
        store
            .cancel_task(
                RequestId::new(),
                owner,
                TaskVersion {
                    task_id: attempt.task_id,
                    revision: 1,
                },
            )
            .unwrap();
        store
            .recover_startup(
                RequestId::new(),
                SessionRef {
                    id: attempt.session_id,
                    generation: 1,
                },
            )
            .unwrap();
        let snapshot = store.snapshot().unwrap();
        let current = snapshot
            .attempts
            .iter()
            .find(|current| current.id == attempt.id)
            .unwrap();
        assert_eq!(
            current.state,
            if starting {
                AttemptState::CancelRequested
            } else {
                AttemptState::Cancelled
            }
        );
        assert_eq!(current.state.holds_slot(), starting);
    }
}

#[tokio::test]
async fn supervisor_recovery_excludes_sessions_created_after_the_endpoint_was_bound() {
    use adroit::control::{client::Client, protocol::Command, service::Service};
    use adroit::settings::Settings;
    use adroit_term::Daemon;
    use std::os::unix::fs::PermissionsExt;
    let dir = tempfile::tempdir().unwrap();
    std::fs::set_permissions(dir.path(), std::fs::Permissions::from_mode(0o700)).unwrap();
    let root = dir.path().join("control");
    let mut store = Store::open(&root.join("state.sqlite3")).unwrap();
    let inherited = store
        .register_leader(
            RequestId::new(),
            LogicalSessionId::new(),
            Agent::Codex,
            dir.path().into(),
        )
        .unwrap()
        .value;
    drop(store);
    let mut service = Service::bind(&root).await.unwrap();
    let client = Client::human(&root).unwrap();
    let new_id = LogicalSessionId::new();
    client
        .request(
            RequestId::new(),
            Command::RegisterLeader {
                id: new_id,
                agent: Agent::Codex,
                cwd: dir.path().into(),
                permissions: AgentPermissions::Default,
            },
        )
        .await
        .unwrap();
    let socket = dir.path().join("d.sock");
    let native = native_daemon::serve_process(socket.clone());
    service
        .supervise(Daemon::new(socket), "/adroit".into(), Settings::default)
        .unwrap();
    tokio::time::timeout(std::time::Duration::from_secs(3), async {
        loop {
            let snapshot = service
                .actor()
                .call(|store| store.snapshot())
                .await
                .unwrap();
            if snapshot.sessions.iter().any(|session| {
                session.reference() == inherited.reference()
                    && session.state == ManagedSessionState::Failed
            }) {
                assert_eq!(
                    snapshot
                        .sessions
                        .iter()
                        .find(|session| session.binding.logical_id() == new_id)
                        .unwrap()
                        .state,
                    ManagedSessionState::Preparing
                );
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        }
    })
    .await
    .unwrap();
    native.abort();
}

#[test]
fn a_late_startup_uncertainty_cannot_revive_a_confirmed_exited_leader() {
    let dir = tempfile::tempdir().unwrap();
    let mut store = Store::open(&dir.path().join("control/state.sqlite3")).unwrap();
    let leader = store
        .register_leader(
            RequestId::new(),
            LogicalSessionId::new(),
            Agent::Codex,
            dir.path().into(),
        )
        .unwrap()
        .value
        .reference();
    let terminal = MessageId::new().to_string().replace('-', "");
    store
        .attach_session(
            RequestId::new(),
            leader,
            dir.path().into(),
            terminal.clone(),
            MessageId::new().to_string(),
        )
        .unwrap();
    store
        .observe_terminal_exit(RequestId::new(), leader, terminal)
        .unwrap();
    store
        .start_uncertain(
            RequestId::new(),
            leader,
            "A delayed transport failure".into(),
        )
        .unwrap();
    assert_eq!(
        store.snapshot().unwrap().sessions[0].state,
        ManagedSessionState::Exited
    );
}

#[test]
fn a_confirmed_gate_abort_after_attach_frees_the_slot_and_cannot_claim_success() {
    let dir = tempfile::tempdir().unwrap();
    let mut store = Store::open(&dir.path().join("control/state.sqlite3")).unwrap();
    let (_, _, attempt, terminal) = running_worker(&mut store, dir.path());
    let worker = SessionRef {
        id: attempt.session_id,
        generation: 1,
    };
    store
        .abort_prepared_start(
            RequestId::new(),
            worker,
            terminal,
            "The ownership response was lost before release".into(),
        )
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
        ManagedSessionState::Failed
    );
    assert!(snapshot.completions.is_empty());
}

#[test]
fn a_gate_abort_of_an_attached_leader_fails_its_unstarted_run() {
    let dir = tempfile::tempdir().unwrap();
    let mut store = Store::open(&dir.path().join("control/state.sqlite3")).unwrap();
    let (owner, _, _, _) = running_worker(&mut store, dir.path());
    let terminal = MessageId::new().to_string().replace('-', "");
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
        .abort_prepared_start(
            RequestId::new(),
            owner,
            terminal,
            "Preparation failed before release".into(),
        )
        .unwrap();
    assert_eq!(store.snapshot().unwrap().runs[0].state, RunState::Failed);
}
