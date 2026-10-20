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
    assert_eq!(snapshot.candidates, vec![prepared.summary()]);
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

#[tokio::test]
async fn pending_read_only_capture_resumes_after_supervisor_restart_without_a_window() {
    use adroit::control::{capture::CaptureSupervisor, service::Service};
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("output.txt"), "Result").unwrap();
    let root = dir.path().join("control");
    let mut store = Store::open(&root.join("state.sqlite3")).unwrap();
    let (owner, run, attempt, _, _) = finished_change(&mut store, dir.path());
    let plan = plan(run, &attempt);
    store
        .propose_candidate(RequestId::new(), owner, 2, plan.clone())
        .unwrap();
    drop(store);
    let service = Service::bind(&root).await.unwrap();
    let mut capture = CaptureSupervisor::new(service.actor());
    assert_eq!(capture.tick().await.unwrap(), vec![plan.id]);
    drop(capture);
    let id = plan.id;
    assert_eq!(
        service
            .actor()
            .call(move |store| store.candidate(id))
            .await
            .unwrap()
            .state,
        CandidateState::Requested
    );
    let mut capture = CaptureSupervisor::new(service.actor());
    tokio::time::timeout(std::time::Duration::from_secs(3), async {
        loop {
            capture.tick().await.unwrap();
            let candidate = service
                .actor()
                .call(move |store| store.candidate(id))
                .await
                .unwrap();
            if candidate.state == CandidateState::Prepared {
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        }
    })
    .await
    .unwrap();
    let snapshot = service
        .actor()
        .call(|store| store.snapshot())
        .await
        .unwrap();
    assert_eq!(snapshot.candidates.len(), 1);
    assert_eq!(snapshot.runs[0].state, RunState::Active);
    assert_eq!(
        snapshot
            .messages
            .iter()
            .filter(|message| message.body.contains("candidate_capture_finished"))
            .count(),
        1
    );
}

#[tokio::test]
async fn one_stale_directory_output_does_not_block_another_runs_capture() {
    use adroit::control::{capture::CaptureSupervisor, service::Service};
    let first = tempfile::tempdir().unwrap();
    let second = tempfile::tempdir().unwrap();
    for dir in [first.path(), second.path()] {
        std::fs::write(dir.join("output.txt"), "Result").unwrap();
    }
    let root = first.path().join("control");
    let mut store = Store::open(&root.join("state.sqlite3")).unwrap();
    let mut ids = Vec::new();
    for dir in [first.path(), second.path()] {
        let (owner, run, attempt, _, _) = finished_change(&mut store, dir);
        let plan = plan(run, &attempt);
        store
            .propose_candidate(RequestId::new(), owner, 2, plan.clone())
            .unwrap();
        ids.push(plan.id);
    }
    std::fs::write(first.path().join("output.txt"), "Changed after reporting").unwrap();
    drop(store);
    let service = Service::bind(&root).await.unwrap();
    let mut capture = CaptureSupervisor::new(service.actor());
    tokio::time::timeout(std::time::Duration::from_secs(3), async {
        loop {
            capture.tick().await.unwrap();
            let snapshot = service
                .actor()
                .call(|store| store.snapshot())
                .await
                .unwrap();
            if snapshot
                .candidates
                .iter()
                .all(|candidate| candidate.state != CandidateState::Requested)
            {
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        }
    })
    .await
    .unwrap();
    let snapshot = service
        .actor()
        .call(|store| store.snapshot())
        .await
        .unwrap();
    assert_eq!(
        snapshot
            .candidates
            .iter()
            .find(|candidate| candidate.plan.id == ids[0])
            .unwrap()
            .state,
        CandidateState::Failed
    );
    assert_eq!(
        snapshot
            .candidates
            .iter()
            .find(|candidate| candidate.plan.id == ids[1])
            .unwrap()
            .state,
        CandidateState::Prepared
    );
}

#[tokio::test]
async fn public_snapshots_summarize_large_file_evidence_instead_of_exceeding_the_frame_limit() {
    use adroit::control::{client::Client, protocol::Command, service::Service};
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("output.txt"), "Result").unwrap();
    let root = dir.path().join("control");
    let mut store = Store::open(&root.join("state.sqlite3")).unwrap();
    let (owner, run, attempt, area, artifact) = finished_change(&mut store, dir.path());
    let plan = plan(run, &attempt);
    store
        .propose_candidate(RequestId::new(), owner, 2, plan.clone())
        .unwrap();
    let mut proof = adroit::candidates::capture_candidate(&area, &artifact, &plan.inputs)
        .await
        .unwrap();
    // This layer receives reader evidence; its public projection must not repeat the full manifest.
    proof.files.files = (0..16384)
        .map(|index| CandidateFile {
            path: format!("captured/file-{index:05}.txt"),
            state: CandidateFileState::File {
                sha256: "a".repeat(64),
                executable: false,
            },
        })
        .collect();
    store
        .record_candidate_capture(RequestId::new(), plan.id, Ok(proof))
        .unwrap();
    let token = store.grant(owner).unwrap();
    drop(store);
    let _service = Service::bind(&root).await.unwrap();
    let client = Client::session(&root, token);
    let value = client
        .request(RequestId::new(), Command::Snapshot)
        .await
        .unwrap();
    assert_eq!(value["candidates"][0]["proof"]["file_count"], 16384);
    assert!(value["candidates"][0]["proof"].get("files").is_none());
    assert!(serde_json::to_vec(&value).unwrap().len() < 64 * 1024);
    client
        .request(
            RequestId::new(),
            Command::ProposeCandidate {
                expected_revision: 3,
                plan: CandidatePlan {
                    id: CandidateId::new(),
                    ..plan
                },
            },
        )
        .await
        .unwrap();
    let events = client
        .request(
            RequestId::new(),
            Command::Events {
                after: 0,
                limit: 1000,
            },
        )
        .await
        .unwrap();
    assert!(serde_json::to_vec(&events).unwrap().len() < 128 * 1024);
    let superseded = events["events"]
        .as_array()
        .unwrap()
        .iter()
        .find(|event| event["kind"] == "candidate_superseded")
        .unwrap();
    assert_eq!(superseded["data"]["proof"]["file_count"], 16384);
}

#[test]
fn cancelling_a_run_invalidates_its_pending_candidate_and_keeps_interactive_sessions_available() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("output.txt"), "Result").unwrap();
    let mut store = Store::open(&dir.path().join("control/state.sqlite3")).unwrap();
    let (owner, run, attempt, _, _) = finished_change(&mut store, dir.path());
    let plan = plan(run, &attempt);
    store
        .propose_candidate(RequestId::new(), owner, 2, plan)
        .unwrap();
    store.cancel_run(RequestId::new(), owner, run).unwrap();
    let snapshot = store.snapshot().unwrap();
    assert_eq!(snapshot.runs[0].state, RunState::Cancelled);
    assert_eq!(snapshot.candidates[0].state, CandidateState::Stale);
    assert_eq!(
        snapshot
            .sessions
            .iter()
            .find(|session| session.binding.logical_id() == attempt.session_id)
            .unwrap()
            .state,
        ManagedSessionState::Running
    );
}
