use adroit::control::store::Store;
use adroit_model::*;
use std::path::Path;

fn leader(store: &mut Store, cwd: &Path) -> (SessionRef, RunId) {
    let session = store
        .register_leader(
            RequestId::new(),
            LogicalSessionId::new(),
            Agent::Codex,
            cwd.into(),
        )
        .unwrap()
        .value;
    let run = RunId::new();
    store
        .open_run(
            RequestId::new(),
            session.reference(),
            run,
            RunBrief {
                title: "Review".into(),
                goal: "Review the requested inputs".into(),
                ..Default::default()
            },
        )
        .unwrap();
    (session.reference(), run)
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
                    instructions: "Review the inputs".into(),
                    ..Default::default()
                },
                dependencies,
            },
        )
        .unwrap();
    id
}

fn running(store: &mut Store, cwd: &Path, terminal: u128) -> (SessionRef, RunId, AttemptRecord) {
    let (owner, run) = leader(store, cwd);
    let id = task(store, owner, run, vec![]);
    let wanted = TaskVersion {
        task_id: id,
        revision: 1,
    };
    store.queue_task(RequestId::new(), owner, wanted).unwrap();
    let attempt = store
        .reserve_attempt(RequestId::new(), owner, wanted, TargetSelection::default())
        .unwrap()
        .value;
    let path = cwd.canonicalize().unwrap();
    let area = store
        .register_work_area(
            RequestId::new(),
            owner,
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
                description: "Source".into(),
                state: WorkAreaState::Ready,
                error: None,
            },
            true,
        )
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
            path,
            format!("{terminal:032x}"),
            MessageId::new().to_string(),
        )
        .unwrap();
    (owner, run, attempt)
}

#[test]
fn cancelling_one_run_keeps_its_slots_until_confirmation_and_leaves_another_run_running() {
    let dir = tempfile::tempdir().unwrap();
    let mut store = Store::open(&dir.path().join("control/state.sqlite3")).unwrap();
    let (first_owner, first, first_attempt) = running(&mut store, dir.path(), 0xab);
    let (_, second, second_attempt) = running(&mut store, dir.path(), 0xcd);
    assert!(
        store
            .cancel_run(RequestId::new(), first_owner, second)
            .is_err()
    );
    let cancelled = store
        .cancel_run(RequestId::new(), first_owner, first)
        .unwrap()
        .value;
    assert_eq!(cancelled.state, RunState::Cancelling);
    let snapshot = store.snapshot().unwrap();
    assert_eq!(
        snapshot
            .attempts
            .iter()
            .find(|attempt| attempt.id == first_attempt.id)
            .unwrap()
            .state,
        AttemptState::CancelRequested
    );
    assert_eq!(
        snapshot
            .attempts
            .iter()
            .find(|attempt| attempt.id == second_attempt.id)
            .unwrap()
            .state,
        AttemptState::Running
    );
    assert_eq!(
        snapshot
            .runs
            .iter()
            .find(|run| run.id == second)
            .unwrap()
            .state,
        RunState::Active
    );
    assert!(
        store
            .open_run(
                RequestId::new(),
                first_owner,
                RunId::new(),
                RunBrief {
                    title: "New run".into(),
                    goal: "Another request".into(),
                    ..Default::default()
                }
            )
            .is_err()
    );
    store
        .report_task(
            RequestId::new(),
            SessionRef {
                id: first_attempt.session_id,
                generation: 1,
            },
            TaskReport {
                message_id: MessageId::new(),
                task: TaskVersion {
                    task_id: first_attempt.task_id,
                    revision: 1,
                },
                attempt_id: first_attempt.id,
                result: TaskResult {
                    status: ReportStatus::Cancelled,
                    summary: "Stopped the assigned work".into(),
                    artifact: None,
                    checks: vec![],
                    unresolved: vec![],
                },
            },
        )
        .unwrap();
    let snapshot = store.snapshot().unwrap();
    assert_eq!(
        snapshot
            .runs
            .iter()
            .find(|run| run.id == first)
            .unwrap()
            .state,
        RunState::Cancelled
    );
    assert_eq!(
        snapshot
            .tasks
            .iter()
            .find(|task| task.id == first_attempt.task_id)
            .unwrap()
            .state,
        TaskState::Cancelled
    );
    assert_eq!(
        snapshot
            .attempts
            .iter()
            .find(|attempt| attempt.id == second_attempt.id)
            .unwrap()
            .state,
        AttemptState::Running
    );
}

#[test]
fn request_updates_invalidate_only_explicitly_affected_tasks_and_shared_dependents_once() {
    let dir = tempfile::tempdir().unwrap();
    let mut store = Store::open(&dir.path().join("control/state.sqlite3")).unwrap();
    let (owner, run) = leader(&mut store, dir.path());
    let first = task(&mut store, owner, run, vec![]);
    let second = task(&mut store, owner, run, vec![]);
    let shared = task(&mut store, owner, run, vec![first, second]);
    let independent = task(&mut store, owner, run, vec![]);
    store
        .queue_task(
            RequestId::new(),
            owner,
            TaskVersion {
                task_id: independent,
                revision: 1,
            },
        )
        .unwrap();
    let updated = store
        .update_run(
            RequestId::new(),
            owner,
            run,
            5,
            RunBrief {
                title: "Review".into(),
                goal: "Review the clarified inputs".into(),
                ..Default::default()
            },
            vec![
                TaskVersion {
                    task_id: first,
                    revision: 1,
                },
                TaskVersion {
                    task_id: second,
                    revision: 1,
                },
            ],
        )
        .unwrap()
        .value;
    assert_eq!(updated.plan_revision, 6);
    let snapshot = store.snapshot().unwrap();
    for id in [first, second, shared] {
        assert_eq!(
            snapshot
                .tasks
                .iter()
                .find(|task| task.id == id)
                .unwrap()
                .revision,
            2
        );
    }
    let task = snapshot
        .tasks
        .iter()
        .find(|task| task.id == independent)
        .unwrap();
    assert_eq!(task.revision, 1);
    assert_eq!(task.state, TaskState::Queued);
}

#[test]
fn an_outdated_request_update_cannot_change_the_brief_or_tasks() {
    let dir = tempfile::tempdir().unwrap();
    let mut store = Store::open(&dir.path().join("control/state.sqlite3")).unwrap();
    let (owner, run) = leader(&mut store, dir.path());
    let id = task(&mut store, owner, run, vec![]);
    let before = store.snapshot().unwrap();
    assert!(
        store
            .update_run(
                RequestId::new(),
                owner,
                run,
                1,
                RunBrief {
                    title: "Wrong".into(),
                    goal: "Outdated change".into(),
                    ..Default::default()
                },
                vec![TaskVersion {
                    task_id: id,
                    revision: 1
                }]
            )
            .is_err()
    );
    assert_eq!(store.snapshot().unwrap(), before);
}

#[test]
fn failed_leader_startup_does_not_override_an_accepted_cancellation() {
    let dir = tempfile::tempdir().unwrap();
    let mut store = Store::open(&dir.path().join("control/state.sqlite3")).unwrap();
    let (owner, run) = leader(&mut store, dir.path());
    assert_eq!(
        store
            .cancel_run(RequestId::new(), owner, run)
            .unwrap()
            .value
            .state,
        RunState::Cancelled
    );
    store
        .fail_start(RequestId::new(), owner, "The provider was not found".into())
        .unwrap();
    assert_eq!(store.snapshot().unwrap().runs[0].state, RunState::Cancelled);
}

#[test]
fn a_failed_run_can_cancel_workers_that_are_still_running() {
    let dir = tempfile::tempdir().unwrap();
    let mut store = Store::open(&dir.path().join("control/state.sqlite3")).unwrap();
    let (owner, run, attempt) = running(&mut store, dir.path(), 0xab);
    store
        .fail_start(
            RequestId::new(),
            owner,
            "The Leader provider was not found".into(),
        )
        .unwrap();
    assert_eq!(store.snapshot().unwrap().runs[0].state, RunState::Failed);
    assert_eq!(
        store
            .cancel_run(RequestId::new(), owner, run)
            .unwrap()
            .value
            .state,
        RunState::Cancelling
    );
    assert_eq!(
        store
            .snapshot()
            .unwrap()
            .attempts
            .iter()
            .find(|item| item.id == attempt.id)
            .unwrap()
            .state,
        AttemptState::CancelRequested
    );
}

#[test]
fn an_affected_task_from_another_run_rolls_back_the_entire_request_update() {
    let dir = tempfile::tempdir().unwrap();
    let mut store = Store::open(&dir.path().join("control/state.sqlite3")).unwrap();
    let (owner, run) = leader(&mut store, dir.path());
    let own_task = task(&mut store, owner, run, vec![]);
    let (other_owner, other_run) = leader(&mut store, dir.path());
    let foreign_task = task(&mut store, other_owner, other_run, vec![]);
    let before = store.snapshot().unwrap();
    assert!(
        store
            .update_run(
                RequestId::new(),
                owner,
                run,
                2,
                RunBrief {
                    title: "Wrong scope".into(),
                    goal: "Must not change either request".into(),
                    ..Default::default()
                },
                vec![
                    TaskVersion {
                        task_id: own_task,
                        revision: 1
                    },
                    TaskVersion {
                        task_id: foreign_task,
                        revision: 1
                    }
                ]
            )
            .is_err()
    );
    assert_eq!(store.snapshot().unwrap(), before);
}

fn report_success(store: &mut Store, attempt: &AttemptRecord) -> MessageId {
    let id = MessageId::new();
    store
        .report_task(
            RequestId::new(),
            SessionRef {
                id: attempt.session_id,
                generation: 1,
            },
            TaskReport {
                message_id: id,
                task: TaskVersion {
                    task_id: attempt.task_id,
                    revision: attempt.task_revision,
                },
                attempt_id: attempt.id,
                result: TaskResult {
                    status: ReportStatus::Succeeded,
                    summary: "Reviewed the inputs".into(),
                    artifact: Some(TaskArtifact::Report {
                        data: serde_json::json!({"findings":[]}),
                        files: vec![],
                        reviewed_revision: None,
                    }),
                    checks: vec![],
                    unresolved: vec![],
                },
            },
        )
        .unwrap();
    id
}

#[test]
fn a_report_run_finishes_with_current_evidence_without_requiring_a_vcs_commit() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("control/state.sqlite3");
    let mut store = Store::open(&path).unwrap();
    let (owner, run, attempt) = running(&mut store, dir.path(), 0xab);
    let report = report_success(&mut store, &attempt);
    assert_eq!(store.snapshot().unwrap().runs[0].state, RunState::Active);
    let request = RequestId::new();
    let completed = store
        .complete_run(request, owner, run, 2, "Review finished".into())
        .unwrap();
    assert_eq!(completed.value.reports, vec![report]);
    assert_eq!(
        store
            .complete_run(request, owner, run, 2, "Review finished".into())
            .unwrap(),
        completed
    );
    drop(store);
    let mut store = Store::open(&path).unwrap();
    let snapshot = store.snapshot().unwrap();
    assert_eq!(snapshot.runs[0].state, RunState::Succeeded);
    assert_eq!(snapshot.completions.len(), 1);
    assert_eq!(snapshot.completions[0].summary, "Review finished");
    assert_eq!(
        snapshot
            .sessions
            .iter()
            .find(|session| session.reference().id == attempt.session_id)
            .unwrap()
            .state,
        ManagedSessionState::Running
    );
}

#[test]
fn a_current_blocking_question_needs_its_addressed_answer_before_completion() {
    let dir = tempfile::tempdir().unwrap();
    let mut store = Store::open(&dir.path().join("control/state.sqlite3")).unwrap();
    let (owner, run, attempt) = running(&mut store, dir.path(), 0xab);
    let question = AgentMessage {
        id: MessageId::new(),
        run_id: run,
        from: attempt.session_id,
        to: owner.id,
        task_id: Some(attempt.task_id),
        attempt_id: Some(attempt.id),
        revision: Some(1),
        kind: MessageKind::Question,
        body: "Should the review include generated files?".into(),
        reply_to: None,
        blocks_work: true,
    };
    store
        .send_message(
            RequestId::new(),
            SessionRef {
                id: attempt.session_id,
                generation: 1,
            },
            question.clone(),
        )
        .unwrap();
    report_success(&mut store, &attempt);
    assert!(
        store
            .complete_run(RequestId::new(), owner, run, 2, "Review finished".into())
            .is_err()
    );
    store
        .send_message(
            RequestId::new(),
            owner,
            AgentMessage {
                id: MessageId::new(),
                run_id: run,
                from: owner.id,
                to: attempt.session_id,
                task_id: Some(attempt.task_id),
                attempt_id: Some(attempt.id),
                revision: Some(1),
                kind: MessageKind::Answer,
                body: "The requested scope excludes generated files".into(),
                reply_to: Some(question.id),
                blocks_work: false,
            },
        )
        .unwrap();
    assert!(
        store
            .complete_run(RequestId::new(), owner, run, 2, "Review finished".into())
            .is_ok()
    );
}

#[test]
fn stale_plan_revisions_and_a_declared_change_run_cannot_use_report_only_completion() {
    let dir = tempfile::tempdir().unwrap();
    let mut store = Store::open(&dir.path().join("control/state.sqlite3")).unwrap();
    let (owner, run, attempt) = running(&mut store, dir.path(), 0xab);
    report_success(&mut store, &attempt);
    assert!(
        store
            .complete_run(RequestId::new(), owner, run, 1, "Old plan".into())
            .is_err()
    );
    store
        .update_run(
            RequestId::new(),
            owner,
            run,
            2,
            RunBrief {
                title: "Change".into(),
                goal: "Deliver the code change".into(),
                completion_kind: CompletionKind::Change,
                ..Default::default()
            },
            vec![],
        )
        .unwrap();
    assert!(
        store
            .complete_run(
                RequestId::new(),
                owner,
                run,
                3,
                "Must verify a change candidate".into()
            )
            .is_err()
    );
    assert!(store.snapshot().unwrap().completions.is_empty());
}

#[test]
fn optional_unstarted_work_can_be_skipped_but_an_active_worker_cannot_be_ignored() {
    let dir = tempfile::tempdir().unwrap();
    let mut store = Store::open(&dir.path().join("control/state.sqlite3")).unwrap();
    let (owner, run, attempt) = running(&mut store, dir.path(), 0xab);
    let optional = TaskId::new();
    store
        .put_task(
            RequestId::new(),
            owner,
            TaskPlan {
                id: optional,
                run_id: run,
                expected_revision: None,
                brief: TaskBrief {
                    instructions: "Optional supplementary review".into(),
                    required: false,
                    ..Default::default()
                },
                dependencies: vec![],
            },
        )
        .unwrap();
    store
        .queue_task(
            RequestId::new(),
            owner,
            TaskVersion {
                task_id: optional,
                revision: 1,
            },
        )
        .unwrap();
    assert!(
        store
            .complete_run(RequestId::new(), owner, run, 3, "Too early".into())
            .is_err()
    );
    report_success(&mut store, &attempt);
    store
        .complete_run(
            RequestId::new(),
            owner,
            run,
            3,
            "Required review finished".into(),
        )
        .unwrap();
    assert_eq!(
        store
            .snapshot()
            .unwrap()
            .tasks
            .iter()
            .find(|task| task.id == optional)
            .unwrap()
            .state,
        TaskState::Cancelled
    );
}

#[test]
fn a_change_artifact_cannot_bypass_verification_by_leaving_the_run_kind_as_report() {
    let dir = tempfile::tempdir().unwrap();
    let mut store = Store::open(&dir.path().join("control/state.sqlite3")).unwrap();
    let (owner, run, attempt) = running(&mut store, dir.path(), 0xab);
    let area = store.snapshot().unwrap().attempts[0].work_area_id.unwrap();
    store
        .report_task(
            RequestId::new(),
            SessionRef {
                id: attempt.session_id,
                generation: 1,
            },
            TaskReport {
                message_id: MessageId::new(),
                task: TaskVersion {
                    task_id: attempt.task_id,
                    revision: 1,
                },
                attempt_id: attempt.id,
                result: TaskResult {
                    status: ReportStatus::Succeeded,
                    summary: "Changed a file".into(),
                    artifact: Some(TaskArtifact::Change {
                        backend: WorkAreaBackend::Directory,
                        work_area_id: area,
                        revision: None,
                        manifest: vec!["result.txt".into()],
                        candidate_fingerprint: Some("declared-result-fingerprint".into()),
                    }),
                    checks: vec![],
                    unresolved: vec![],
                },
            },
        )
        .unwrap();
    assert!(
        store
            .complete_run(RequestId::new(), owner, run, 2, "Change finished".into())
            .is_err()
    );
    assert_eq!(store.snapshot().unwrap().runs[0].state, RunState::Active);
}

#[test]
fn an_answer_without_a_question_reference_is_refused_without_creating_a_message() {
    let dir = tempfile::tempdir().unwrap();
    let mut store = Store::open(&dir.path().join("control/state.sqlite3")).unwrap();
    let (owner, run, attempt) = running(&mut store, dir.path(), 0xab);
    let before = store.snapshot().unwrap();
    assert!(
        store
            .send_message(
                RequestId::new(),
                owner,
                AgentMessage {
                    id: MessageId::new(),
                    run_id: run,
                    from: owner.id,
                    to: attempt.session_id,
                    task_id: Some(attempt.task_id),
                    attempt_id: Some(attempt.id),
                    revision: Some(1),
                    kind: MessageKind::Answer,
                    body: "Assumed answer".into(),
                    reply_to: None,
                    blocks_work: false
                }
            )
            .is_err()
    );
    assert_eq!(store.snapshot().unwrap(), before);
}
