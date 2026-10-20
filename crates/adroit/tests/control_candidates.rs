use adroit::control::store::Store;
use adroit_model::*;
use std::path::Path;

fn finished_change(
    store: &mut Store,
    path: &Path,
) -> (SessionRef, RunId, AttemptRecord, WorkArea, TaskArtifact) {
    let path = path.canonicalize().unwrap();
    let owner = store
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
            owner,
            run,
            RunBrief {
                title: "Change".into(),
                goal: "Deliver the verified change".into(),
                completion_kind: CompletionKind::Change,
                quality_gate: QualityGate {
                    commands: vec![vec!["just".into(), "ci".into()]],
                    review_required: false,
                },
                ..Default::default()
            },
        )
        .unwrap();
    let task = TaskVersion {
        task_id: TaskId::new(),
        revision: 1,
    };
    store
        .put_task(
            RequestId::new(),
            owner,
            TaskPlan {
                id: task.task_id,
                run_id: run,
                expected_revision: None,
                brief: TaskBrief {
                    instructions: "Implement the change".into(),
                    purpose: TaskPurpose::Implement,
                    ..Default::default()
                },
                dependencies: vec![],
            },
        )
        .unwrap();
    store.queue_task(RequestId::new(), owner, task).unwrap();
    let attempt = store
        .reserve_attempt(RequestId::new(), owner, task, TargetSelection::default())
        .unwrap()
        .value;
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
                description: "Candidate".into(),
                state: WorkAreaState::Ready,
                error: None,
            },
            true,
        )
        .unwrap()
        .value;
    store
        .bind_work_area(RequestId::new(), owner, task, attempt.id, area.id)
        .unwrap();
    store
        .begin_start(RequestId::new(), owner, task, attempt.id)
        .unwrap();
    let worker = SessionRef {
        id: attempt.session_id,
        generation: 1,
    };
    store
        .attach_session(
            RequestId::new(),
            worker,
            path.clone(),
            attempt.id.to_string().replace('-', ""),
            MessageId::new().to_string(),
        )
        .unwrap();
    let artifact = TaskArtifact::Change {
        backend: WorkAreaBackend::Directory,
        work_area_id: area.id,
        revision: None,
        manifest: vec!["output.txt".into()],
        candidate_fingerprint: Some(
            adroit::candidates::capture_files(&path, &["output.txt".into()])
                .unwrap()
                .fingerprint,
        ),
    };
    store
        .report_task(
            RequestId::new(),
            worker,
            TaskReport {
                message_id: MessageId::new(),
                task,
                attempt_id: attempt.id,
                result: TaskResult {
                    status: ReportStatus::Succeeded,
                    summary: "Implemented the output".into(),
                    artifact: Some(artifact.clone()),
                    checks: vec![],
                    unresolved: vec![],
                },
            },
        )
        .unwrap();
    (owner, run, attempt, area, artifact)
}

fn plan(run: RunId, attempt: &AttemptRecord) -> CandidatePlan {
    CandidatePlan {
        id: CandidateId::new(),
        run_id: run,
        source: TaskVersion {
            task_id: attempt.task_id,
            revision: 1,
        },
        attempt_id: attempt.id,
        inputs: vec!["rules.txt".into()],
    }
}

#[tokio::test]
async fn a_candidate_proposal_and_its_exact_capture_survive_reopening_without_completing_the_run() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("output.txt"), "Result").unwrap();
    std::fs::write(dir.path().join("rules.txt"), "Rules").unwrap();
    let path = dir.path().join("control/state.sqlite3");
    let mut store = Store::open(&path).unwrap();
    let (owner, run, attempt, area, artifact) = finished_change(&mut store, dir.path());
    let plan = plan(run, &attempt);
    let request = RequestId::new();
    let requested = store
        .propose_candidate(request, owner, 2, plan.clone())
        .unwrap();
    assert_eq!(requested.value.state, CandidateState::Requested);
    assert!(requested.value.proof.is_none());
    assert_eq!(
        store
            .propose_candidate(request, owner, 2, plan.clone())
            .unwrap(),
        requested
    );
    let proof = adroit::candidates::capture_candidate(&area, &artifact, &plan.inputs)
        .await
        .unwrap();
    let prepared = store
        .record_candidate_capture(RequestId::new(), plan.id, Ok(proof.clone()))
        .unwrap()
        .value;
    assert_eq!(prepared.state, CandidateState::Prepared);
    assert_eq!(prepared.proof, Some(proof));
    assert!(
        store
            .complete_run(RequestId::new(), owner, run, 3, "Unverified change".into())
            .is_err()
    );
    drop(store);
    let mut store = Store::open(&path).unwrap();
    let snapshot = store.snapshot().unwrap();
    assert_eq!(snapshot.candidates, vec![prepared]);
    assert_eq!(snapshot.runs[0].candidate_id, Some(plan.id));
    assert_eq!(snapshot.runs[0].state, RunState::Active);
    assert_eq!(snapshot.runs[0].plan_revision, 3);
}

#[tokio::test]
async fn changing_the_source_invalidates_a_pending_capture_and_cannot_restore_it_to_prepared() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("output.txt"), "Result").unwrap();
    let mut store = Store::open(&dir.path().join("control/state.sqlite3")).unwrap();
    let (owner, run, attempt, area, artifact) = finished_change(&mut store, dir.path());
    let plan = plan(run, &attempt);
    store
        .propose_candidate(RequestId::new(), owner, 2, plan.clone())
        .unwrap();
    store
        .put_task(
            RequestId::new(),
            owner,
            TaskPlan {
                id: attempt.task_id,
                run_id: run,
                expected_revision: Some(1),
                brief: TaskBrief {
                    instructions: "Revised implementation".into(),
                    ..Default::default()
                },
                dependencies: vec![],
            },
        )
        .unwrap();
    assert_eq!(
        store.snapshot().unwrap().candidates[0].state,
        CandidateState::Stale
    );
    let proof = adroit::candidates::capture_candidate(&area, &artifact, &plan.inputs)
        .await
        .unwrap();
    let result = store
        .record_candidate_capture(RequestId::new(), plan.id, Ok(proof))
        .unwrap()
        .value;
    assert_eq!(result.state, CandidateState::Stale);
    assert!(result.proof.is_none());
}

#[test]
fn selecting_a_second_candidate_supersedes_the_first_without_reassigning_task_results() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("output.txt"), "Result").unwrap();
    let mut store = Store::open(&dir.path().join("control/state.sqlite3")).unwrap();
    let (owner, run, attempt, _, _) = finished_change(&mut store, dir.path());
    let first = plan(run, &attempt);
    let second = plan(run, &attempt);
    store
        .propose_candidate(RequestId::new(), owner, 2, first.clone())
        .unwrap();
    store
        .propose_candidate(RequestId::new(), owner, 3, second.clone())
        .unwrap();
    let snapshot = store.snapshot().unwrap();
    assert_eq!(snapshot.candidates[0].state, CandidateState::Superseded);
    assert_eq!(snapshot.candidates[1].state, CandidateState::Requested);
    assert_eq!(snapshot.runs[0].candidate_id, Some(second.id));
    assert_eq!(snapshot.tasks[0].state, TaskState::Succeeded);
    assert_eq!(snapshot.reports.len(), 1);
    assert!(
        store
            .propose_candidate(RequestId::new(), owner, 2, first)
            .is_ok()
    );
    assert_eq!(
        store.snapshot().unwrap().runs[0].candidate_id,
        Some(second.id)
    );
}

#[test]
fn stale_plan_revisions_and_foreign_results_cannot_select_a_candidate() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("output.txt"), "Result").unwrap();
    let mut store = Store::open(&dir.path().join("control/state.sqlite3")).unwrap();
    let (owner, run, attempt, _, _) = finished_change(&mut store, dir.path());
    let (other, other_run, other_attempt, _, _) = finished_change(&mut store, dir.path());
    let before = store.snapshot().unwrap();
    assert!(
        store
            .propose_candidate(RequestId::new(), owner, 1, plan(run, &attempt))
            .is_err()
    );
    assert!(
        store
            .propose_candidate(RequestId::new(), owner, 2, plan(other_run, &other_attempt))
            .is_err()
    );
    assert!(
        store
            .propose_candidate(RequestId::new(), other, 2, plan(other_run, &attempt))
            .is_err()
    );
    assert_eq!(store.snapshot().unwrap(), before);
}
