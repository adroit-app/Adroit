mod support;
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

fn quality_plan(
    run: RunId,
    source: &AttemptRecord,
    area: WorkAreaId,
    candidate: CandidateId,
) -> TaskPlan {
    TaskPlan {
        id: TaskId::new(),
        run_id: run,
        expected_revision: None,
        brief: TaskBrief {
            instructions: "Run the declared checks on this candidate".into(),
            purpose: TaskPurpose::Verify,
            candidate_id: Some(candidate),
            workdir: WorkdirPolicy::Existing(area),
            target: TargetPreference {
                agent: Some(Agent::Codex),
                permissions: AgentPermissions::ReadOnly,
                ..Default::default()
            },
            ..Default::default()
        },
        dependencies: vec![source.task_id],
    }
}

#[tokio::test]
async fn a_quality_worker_can_queue_early_but_needs_captured_candidate_inputs_before_starting() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("output.txt"), "Result").unwrap();
    let mut store = Store::open(&dir.path().join("control/state.sqlite3")).unwrap();
    let (owner, run, source, area, artifact) = finished_change(&mut store, dir.path());
    let candidate = plan(run, &source);
    store
        .propose_candidate(RequestId::new(), owner, 2, candidate.clone())
        .unwrap();
    let task = store
        .put_task(
            RequestId::new(),
            owner,
            quality_plan(run, &source, area.id, candidate.id),
        )
        .unwrap()
        .value;
    let wanted = TaskVersion {
        task_id: task.id,
        revision: 1,
    };
    store.queue_task(RequestId::new(), owner, wanted).unwrap();
    assert!(
        store
            .reserve_attempt(RequestId::new(), owner, wanted, TargetSelection::default())
            .is_err()
    );
    let proof = adroit::candidates::capture_candidate(&area, &artifact, &candidate.inputs)
        .await
        .unwrap();
    store
        .record_candidate_capture(RequestId::new(), candidate.id, Ok(proof.clone()))
        .unwrap();
    let attempt = store
        .reserve_attempt(RequestId::new(), owner, wanted, TargetSelection::default())
        .unwrap()
        .value;
    store
        .bind_work_area(RequestId::new(), owner, wanted, attempt.id, area.id)
        .unwrap();
    assert!(
        store
            .begin_start(RequestId::new(), owner, wanted, attempt.id)
            .is_err()
    );
    let recorded = store
        .record_candidate_input(RequestId::new(), owner, wanted, attempt.id, Ok(proof))
        .unwrap()
        .value
        .unwrap();
    assert!(recorded.candidate_input.is_some());
    assert_eq!(
        store
            .begin_start(RequestId::new(), owner, wanted, attempt.id)
            .unwrap()
            .value
            .state,
        AttemptState::Starting
    );
}

#[tokio::test]
async fn a_changed_verification_input_invalidates_the_candidate_without_releasing_the_program() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("output.txt"), "Result").unwrap();
    std::fs::write(dir.path().join("rules.txt"), "Original rules").unwrap();
    let mut store = Store::open(&dir.path().join("control/state.sqlite3")).unwrap();
    let (owner, run, source, area, artifact) = finished_change(&mut store, dir.path());
    let candidate = plan(run, &source);
    store
        .propose_candidate(RequestId::new(), owner, 2, candidate.clone())
        .unwrap();
    let proof = adroit::candidates::capture_candidate(&area, &artifact, &candidate.inputs)
        .await
        .unwrap();
    store
        .record_candidate_capture(RequestId::new(), candidate.id, Ok(proof))
        .unwrap();
    let task = store
        .put_task(
            RequestId::new(),
            owner,
            quality_plan(run, &source, area.id, candidate.id),
        )
        .unwrap()
        .value;
    let wanted = TaskVersion {
        task_id: task.id,
        revision: 1,
    };
    store.queue_task(RequestId::new(), owner, wanted).unwrap();
    let attempt = store
        .reserve_attempt(RequestId::new(), owner, wanted, TargetSelection::default())
        .unwrap()
        .value;
    store
        .bind_work_area(RequestId::new(), owner, wanted, attempt.id, area.id)
        .unwrap();
    std::fs::write(dir.path().join("rules.txt"), "New rules").unwrap();
    let changed = adroit::candidates::capture_candidate(&area, &artifact, &candidate.inputs)
        .await
        .unwrap();
    assert!(
        store
            .record_candidate_input(RequestId::new(), owner, wanted, attempt.id, Ok(changed))
            .unwrap()
            .value
            .is_none()
    );
    assert_eq!(
        store.snapshot().unwrap().candidates[0].state,
        CandidateState::Stale
    );
    assert!(
        store
            .begin_start(RequestId::new(), owner, wanted, attempt.id)
            .is_err()
    );
}

#[test]
fn candidate_checks_cannot_use_another_run_a_fresh_working_area_or_implementation_purpose() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("output.txt"), "Result").unwrap();
    let mut store = Store::open(&dir.path().join("control/state.sqlite3")).unwrap();
    let (owner, run, source, area, _) = finished_change(&mut store, dir.path());
    let candidate = plan(run, &source);
    store
        .propose_candidate(RequestId::new(), owner, 2, candidate.clone())
        .unwrap();
    let (other, other_run, other_source, _, _) = finished_change(&mut store, dir.path());
    let before = store.snapshot().unwrap();
    assert!(
        store
            .put_task(
                RequestId::new(),
                other,
                quality_plan(other_run, &other_source, area.id, candidate.id)
            )
            .is_err()
    );
    for purpose in [TaskPurpose::Review, TaskPurpose::Verify] {
        for permissions in [AgentPermissions::Default, AgentPermissions::ReadOnly] {
            let mut automatic = quality_plan(run, &source, area.id, candidate.id);
            automatic.brief.purpose = purpose;
            automatic.brief.target.permissions = permissions;
            automatic.brief.workdir = WorkdirPolicy::Auto;
            assert!(store.put_task(RequestId::new(), owner, automatic).is_err());
        }
    }
    let mut implementation = quality_plan(run, &source, area.id, candidate.id);
    implementation.brief.purpose = TaskPurpose::Implement;
    assert!(
        store
            .put_task(RequestId::new(), owner, implementation)
            .is_err()
    );
    let mut detached = quality_plan(run, &source, area.id, candidate.id);
    detached.dependencies.clear();
    assert!(store.put_task(RequestId::new(), owner, detached).is_err());
    assert_eq!(store.snapshot().unwrap(), before);
}

#[tokio::test]
async fn waiting_for_candidate_capture_does_not_block_unrelated_ready_work() {
    use adroit::control::{scheduler::Scheduler, service::Service};
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("output.txt"), "Result").unwrap();
    let root = dir.path().join("control");
    let mut store = Store::open(&root.join("state.sqlite3")).unwrap();
    let (owner, run, source, area, _) = finished_change(&mut store, dir.path());
    let candidate = plan(run, &source);
    store
        .propose_candidate(RequestId::new(), owner, 2, candidate.clone())
        .unwrap();
    let check = store
        .put_task(
            RequestId::new(),
            owner,
            quality_plan(run, &source, area.id, candidate.id),
        )
        .unwrap()
        .value;
    store
        .queue_task(
            RequestId::new(),
            owner,
            TaskVersion {
                task_id: check.id,
                revision: 1,
            },
        )
        .unwrap();
    let ready = TaskId::new();
    store
        .put_task(
            RequestId::new(),
            owner,
            TaskPlan {
                id: ready,
                run_id: run,
                expected_revision: None,
                brief: TaskBrief {
                    instructions: "Classify unrelated input".into(),
                    target: TargetPreference {
                        agent: Some(Agent::Codex),
                        ..Default::default()
                    },
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
                task_id: ready,
                revision: 1,
            },
        )
        .unwrap();
    drop(store);
    let service = Service::bind(&root).await.unwrap();
    let mut scheduler = Scheduler::new(
        service.actor(),
        adroit_term::Daemon::new(dir.path().join("not-running.sock")),
        root,
        "/adroit".into(),
    );
    let started = scheduler
        .tick(adroit::settings::Settings::default(), 4)
        .await
        .unwrap();
    assert_eq!(started.len(), 1);
    let snapshot = service
        .actor()
        .call(|store| store.snapshot())
        .await
        .unwrap();
    let attempt = snapshot
        .attempts
        .iter()
        .find(|attempt| attempt.id == started[0])
        .unwrap();
    assert_eq!(attempt.task_id, ready);
    assert!(
        snapshot
            .tasks
            .iter()
            .find(|task| task.id == check.id)
            .unwrap()
            .current_attempt
            .is_none()
    );
}

#[tokio::test]
async fn the_native_program_starts_only_after_a_matching_candidate_preflight() {
    use adroit::control::{scheduler::Scheduler, service::Service};
    use adroit_term::Daemon;
    for changed in [false, true] {
        let dir = tempfile::tempdir().unwrap();
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(dir.path(), std::fs::Permissions::from_mode(0o700)).unwrap();
        std::fs::write(dir.path().join("output.txt"), "Result").unwrap();
        std::fs::write(dir.path().join("rules.txt"), "Original rules").unwrap();
        let root = dir.path().join("control");
        let mut store = Store::open(&root.join("state.sqlite3")).unwrap();
        let (owner, run, source, area, artifact) = finished_change(&mut store, dir.path());
        let candidate = plan(run, &source);
        store
            .propose_candidate(RequestId::new(), owner, 2, candidate.clone())
            .unwrap();
        let proof = adroit::candidates::capture_candidate(&area, &artifact, &candidate.inputs)
            .await
            .unwrap();
        store
            .record_candidate_capture(RequestId::new(), candidate.id, Ok(proof))
            .unwrap();
        let task = store
            .put_task(
                RequestId::new(),
                owner,
                quality_plan(run, &source, area.id, candidate.id),
            )
            .unwrap()
            .value;
        store
            .queue_task(
                RequestId::new(),
                owner,
                TaskVersion {
                    task_id: task.id,
                    revision: 1,
                },
            )
            .unwrap();
        drop(store);
        if changed {
            std::fs::write(dir.path().join("rules.txt"), "Changed rules").unwrap();
        }
        let service = Service::bind(&root).await.unwrap();
        let socket = dir.path().join("terminal.sock");
        let daemon_task = support::serve_process(socket.clone());
        let daemon = Daemon::new(socket);
        for _ in 0..100 {
            if daemon.sessions().await.is_ok() {
                break;
            }
            assert!(
                !daemon_task.is_finished(),
                "terminal daemon failed: {:?}",
                daemon_task.await
            );
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        }
        assert!(
            daemon.sessions().await.is_ok(),
            "terminal daemon must be ready before scheduling"
        );
        let marker = dir.path().join("program-started");
        let mut settings = adroit::settings::Settings::default();
        settings.codex.command = vec![
            "/bin/sh".into(),
            "-c".into(),
            support::codex_script(
                "printf started > \"$ADROIT_TEST_START_MARKER\"; sleep 10",
                None,
            ),
            "quality-worker".into(),
        ];
        settings.codex.env.insert(
            "ADROIT_TEST_START_MARKER".into(),
            marker.to_string_lossy().into_owned(),
        );
        let mut scheduler = Scheduler::new(service.actor(), daemon.clone(), root, "/adroit".into());
        assert_eq!(scheduler.tick(settings, 4).await.unwrap().len(), 1);
        let observed = tokio::time::timeout(std::time::Duration::from_secs(5), async {
            loop {
                let snapshot = service
                    .actor()
                    .call(|store| store.snapshot())
                    .await
                    .unwrap();
                let state = snapshot
                    .tasks
                    .iter()
                    .find(|current| current.id == task.id)
                    .unwrap()
                    .state;
                if (changed && state == TaskState::Failed) || (!changed && marker.exists()) {
                    break;
                }
                tokio::time::sleep(std::time::Duration::from_millis(10)).await;
            }
        })
        .await;
        let snapshot = service
            .actor()
            .call(|store| store.snapshot())
            .await
            .unwrap();
        assert!(
            observed.is_ok(),
            "changed={changed}, snapshot={snapshot:?}, events={:?}",
            service
                .actor()
                .call(|store| store.events_after(0, 100))
                .await
                .unwrap()
        );
        let attempt = snapshot
            .attempts
            .iter()
            .find(|attempt| attempt.task_id == task.id)
            .unwrap();
        assert_eq!(attempt.work_area_id, Some(area.id));
        assert_eq!(attempt.target.permissions, AgentPermissions::ReadOnly);
        if changed {
            assert!(!marker.exists());
            assert_eq!(attempt.state, AttemptState::Failed);
            assert_eq!(snapshot.candidates[0].state, CandidateState::Stale);
            assert!(daemon.sessions().await.unwrap().is_empty());
        } else {
            assert!(attempt.candidate_input.is_some());
            assert_eq!(attempt.state, AttemptState::Running);
            assert_eq!(daemon.sessions().await.unwrap().len(), 1);
        }
        drop(scheduler);
        for session in daemon.sessions().await.unwrap() {
            daemon.end(session.id).await.unwrap();
        }
        daemon_task.abort();
    }
}

async fn running_quality_check(
    store: &mut Store,
    path: &Path,
    purpose: TaskPurpose,
) -> (
    SessionRef,
    RunId,
    CandidateId,
    AttemptRecord,
    CandidateProof,
) {
    let (owner, run, source, area, artifact) = finished_change(store, path);
    let candidate = plan(run, &source);
    store
        .propose_candidate(RequestId::new(), owner, 2, candidate.clone())
        .unwrap();
    let proof = adroit::candidates::capture_candidate(&area, &artifact, &candidate.inputs)
        .await
        .unwrap();
    store
        .record_candidate_capture(RequestId::new(), candidate.id, Ok(proof.clone()))
        .unwrap();
    let mut plan = quality_plan(run, &source, area.id, candidate.id);
    plan.brief.purpose = purpose;
    let task = store.put_task(RequestId::new(), owner, plan).unwrap().value;
    let wanted = TaskVersion {
        task_id: task.id,
        revision: 1,
    };
    store.queue_task(RequestId::new(), owner, wanted).unwrap();
    let attempt = store
        .reserve_attempt(
            RequestId::new(),
            owner,
            wanted,
            TargetSelection {
                agent: Agent::Codex,
                ..Default::default()
            },
        )
        .unwrap()
        .value;
    store
        .bind_work_area(RequestId::new(), owner, wanted, attempt.id, area.id)
        .unwrap();
    store
        .record_candidate_input(
            RequestId::new(),
            owner,
            wanted,
            attempt.id,
            Ok(proof.clone()),
        )
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
            area.path,
            attempt.id.to_string().replace('-', ""),
            MessageId::new().to_string(),
        )
        .unwrap();
    (owner, run, candidate.id, attempt, proof)
}

fn check_report(attempt: &AttemptRecord, candidate: CandidateId) -> TaskReport {
    TaskReport {
        message_id: MessageId::new(),
        task: TaskVersion {
            task_id: attempt.task_id,
            revision: attempt.task_revision,
        },
        attempt_id: attempt.id,
        result: TaskResult {
            status: ReportStatus::Succeeded,
            summary: "The quality command passed".into(),
            artifact: Some(TaskArtifact::Report {
                data: serde_json::json!({"checks":"passed"}),
                files: vec![],
                reviewed_revision: None,
            }),
            checks: vec![ResultCheck {
                command: vec!["just".into(), "ci".into()],
                exit_code: 0,
                candidate: candidate.to_string(),
            }],
            unresolved: vec![],
        },
    }
}

#[tokio::test]
async fn a_successful_quality_report_waits_for_post_check_input_confirmation() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("output.txt"), "Result").unwrap();
    let mut store = Store::open(&dir.path().join("control/state.sqlite3")).unwrap();
    let (_, _, candidate, attempt, proof) =
        running_quality_check(&mut store, dir.path(), TaskPurpose::Verify).await;
    store
        .report_task(
            RequestId::new(),
            SessionRef {
                id: attempt.session_id,
                generation: 1,
            },
            check_report(&attempt, candidate),
        )
        .unwrap();
    let snapshot = store.snapshot().unwrap();
    assert_eq!(
        snapshot
            .tasks
            .iter()
            .find(|task| task.id == attempt.task_id)
            .unwrap()
            .state,
        TaskState::Verifying
    );
    assert_eq!(snapshot.candidates[0].state, CandidateState::Prepared);
    assert_eq!(
        snapshot.candidate_checks[0].state,
        CandidateCheckState::Pending
    );
    let checked = store
        .record_candidate_check(RequestId::new(), attempt.id, Ok(proof))
        .unwrap()
        .value;
    assert_eq!(checked.state, CandidateCheckState::Validated);
    let snapshot = store.snapshot().unwrap();
    assert_eq!(
        snapshot
            .tasks
            .iter()
            .find(|task| task.id == attempt.task_id)
            .unwrap()
            .state,
        TaskState::Succeeded
    );
    assert_eq!(snapshot.candidates[0].state, CandidateState::Verified);
}

#[tokio::test]
async fn a_report_for_changed_inputs_cannot_validate_now_or_when_the_files_are_later_restored() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("output.txt"), "Result").unwrap();
    std::fs::write(dir.path().join("rules.txt"), "Original rules").unwrap();
    let mut store = Store::open(&dir.path().join("control/state.sqlite3")).unwrap();
    let (_, _, candidate, attempt, original) =
        running_quality_check(&mut store, dir.path(), TaskPurpose::Verify).await;
    store
        .report_task(
            RequestId::new(),
            SessionRef {
                id: attempt.session_id,
                generation: 1,
            },
            check_report(&attempt, candidate),
        )
        .unwrap();
    let record = store.candidate(candidate).unwrap();
    let area = store.work_area(original.work_area_id).unwrap();
    std::fs::write(dir.path().join("rules.txt"), "Changed during verification").unwrap();
    let changed =
        adroit::candidates::capture_candidate(&area, &record.artifact, &record.plan.inputs)
            .await
            .unwrap();
    assert_eq!(
        store
            .record_candidate_check(RequestId::new(), attempt.id, Ok(changed))
            .unwrap()
            .value
            .state,
        CandidateCheckState::Rejected
    );
    std::fs::write(dir.path().join("rules.txt"), "Original rules").unwrap();
    assert_eq!(
        store
            .record_candidate_check(RequestId::new(), attempt.id, Ok(original))
            .unwrap()
            .value
            .state,
        CandidateCheckState::Rejected
    );
    let snapshot = store.snapshot().unwrap();
    assert_eq!(snapshot.candidates[0].state, CandidateState::Stale);
    assert_eq!(
        snapshot
            .tasks
            .iter()
            .find(|task| task.id == attempt.task_id)
            .unwrap()
            .state,
        TaskState::Failed
    );
}

#[tokio::test]
async fn a_completed_review_requesting_changes_is_not_candidate_approval() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("output.txt"), "Result").unwrap();
    let mut store = Store::open(&dir.path().join("control/state.sqlite3")).unwrap();
    let (owner, run, candidate, attempt, proof) =
        running_quality_check(&mut store, dir.path(), TaskPurpose::Review).await;
    let current = store.snapshot().unwrap().runs[0].clone();
    store
        .update_run(
            RequestId::new(),
            owner,
            run,
            current.plan_revision,
            RunBrief {
                quality_gate: QualityGate {
                    commands: vec![],
                    review_required: true,
                },
                ..current.brief
            },
            vec![],
        )
        .unwrap();
    let mut report = check_report(&attempt, candidate);
    report.result.checks.clear();
    report.result.summary = "Review completed with one issue".into();
    report.result.artifact = Some(TaskArtifact::Review {
        decision: ReviewDecision::ChangesRequested,
        data: serde_json::json!({"findings":["The output does not meet the goal"]}),
        files: vec![],
        reviewed_revision: None,
    });
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
    store
        .record_candidate_check(RequestId::new(), attempt.id, Ok(proof))
        .unwrap();
    let snapshot = store.snapshot().unwrap();
    assert_eq!(
        snapshot
            .tasks
            .iter()
            .find(|task| task.id == attempt.task_id)
            .unwrap()
            .state,
        TaskState::Succeeded
    );
    assert_eq!(snapshot.candidates[0].state, CandidateState::Prepared);
}

#[tokio::test]
async fn a_failed_check_cannot_be_disguised_as_success_or_attached_to_another_candidate() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("output.txt"), "Result").unwrap();
    let mut store = Store::open(&dir.path().join("control/state.sqlite3")).unwrap();
    let (_, _, candidate, attempt, _) =
        running_quality_check(&mut store, dir.path(), TaskPurpose::Verify).await;
    let mut failed = check_report(&attempt, candidate);
    failed.result.checks[0].exit_code = 1;
    assert!(
        store
            .report_task(
                RequestId::new(),
                SessionRef {
                    id: attempt.session_id,
                    generation: 1
                },
                failed
            )
            .is_err()
    );
    let mut foreign = check_report(&attempt, candidate);
    foreign.result.checks[0].candidate = CandidateId::new().to_string();
    assert!(
        store
            .report_task(
                RequestId::new(),
                SessionRef {
                    id: attempt.session_id,
                    generation: 1
                },
                foreign
            )
            .is_err()
    );
    assert!(store.snapshot().unwrap().candidate_checks.is_empty());
    assert_eq!(
        store
            .snapshot()
            .unwrap()
            .tasks
            .iter()
            .find(|task| task.id == attempt.task_id)
            .unwrap()
            .state,
        TaskState::Running
    );
}

#[tokio::test]
async fn pending_quality_confirmation_resumes_after_restart_without_a_leader_turn() {
    use adroit::control::{capture::CaptureSupervisor, service::Service};
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("output.txt"), "Result").unwrap();
    let root = dir.path().join("control");
    let mut store = Store::open(&root.join("state.sqlite3")).unwrap();
    let (_, _, candidate, attempt, _) =
        running_quality_check(&mut store, dir.path(), TaskPurpose::Verify).await;
    store
        .report_task(
            RequestId::new(),
            SessionRef {
                id: attempt.session_id,
                generation: 1,
            },
            check_report(&attempt, candidate),
        )
        .unwrap();
    drop(store);
    let service = Service::bind(&root).await.unwrap();
    let mut capture = CaptureSupervisor::new(service.actor());
    capture.tick().await.unwrap();
    drop(capture);
    let mut capture = CaptureSupervisor::new(service.actor());
    tokio::time::timeout(std::time::Duration::from_secs(3), async {
        loop {
            capture.tick().await.unwrap();
            let snapshot = service
                .actor()
                .call(|store| store.snapshot())
                .await
                .unwrap();
            if snapshot.candidate_checks[0].state == CandidateCheckState::Validated {
                assert_eq!(snapshot.candidates[0].state, CandidateState::Verified);
                assert_eq!(
                    snapshot
                        .tasks
                        .iter()
                        .find(|task| task.id == attempt.task_id)
                        .unwrap()
                        .state,
                    TaskState::Succeeded
                );
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        }
    })
    .await
    .unwrap();
}

#[tokio::test]
async fn a_scoped_snapshot_never_exposes_another_runs_quality_check() {
    use adroit::control::{client::Client, protocol::Command, service::Service};
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("output.txt"), "Result").unwrap();
    let root = dir.path().join("control");
    let mut store = Store::open(&root.join("state.sqlite3")).unwrap();
    let mut clients = Vec::new();
    for _ in 0..2 {
        let (owner, _, candidate, attempt, _) =
            running_quality_check(&mut store, dir.path(), TaskPurpose::Verify).await;
        store
            .report_task(
                RequestId::new(),
                SessionRef {
                    id: attempt.session_id,
                    generation: 1,
                },
                check_report(&attempt, candidate),
            )
            .unwrap();
        clients.push((
            Client::session(&root, store.grant(owner).unwrap()),
            candidate,
        ));
    }
    assert_eq!(store.snapshot().unwrap().candidate_checks.len(), 2);
    drop(store);
    let _service = Service::bind(&root).await.unwrap();
    for (client, candidate) in clients {
        let snapshot = client
            .request(RequestId::new(), Command::Snapshot)
            .await
            .unwrap();
        let checks = snapshot["candidate_checks"].as_array().unwrap();
        assert_eq!(checks.len(), 1);
        assert_eq!(checks[0]["candidate_id"], candidate.to_string());
    }
}

async fn verified_candidate(
    store: &mut Store,
    path: &Path,
) -> (SessionRef, RunId, CandidateId, CandidateProof) {
    let (owner, run, candidate, attempt, proof) =
        running_quality_check(store, path, TaskPurpose::Verify).await;
    store
        .report_task(
            RequestId::new(),
            SessionRef {
                id: attempt.session_id,
                generation: 1,
            },
            check_report(&attempt, candidate),
        )
        .unwrap();
    store
        .record_candidate_check(RequestId::new(), attempt.id, Ok(proof.clone()))
        .unwrap();
    (owner, run, candidate, proof)
}

#[tokio::test]
async fn change_completion_waits_for_a_durable_final_input_check_without_closing_sessions() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("output.txt"), "Result").unwrap();
    let path = dir.path().join("control/state.sqlite3");
    let mut store = Store::open(&path).unwrap();
    let (owner, run, candidate, proof) = verified_candidate(&mut store, dir.path()).await;
    let revision = store.snapshot().unwrap().runs[0].plan_revision;
    let ticket = store
        .request_change_completion(
            RequestId::new(),
            owner,
            run,
            revision,
            candidate,
            "Implemented and verified".into(),
        )
        .unwrap()
        .value;
    assert_eq!(ticket.state, CompletionRequestState::Pending);
    assert!(store.snapshot().unwrap().completions.is_empty());
    drop(store);
    let mut store = Store::open(&path).unwrap();
    assert_eq!(store.pending_completions(64).unwrap(), vec![ticket.clone()]);
    let confirmed = store
        .record_change_completion(RequestId::new(), ticket.id, Ok(proof))
        .unwrap()
        .value;
    assert_eq!(confirmed.state, CompletionRequestState::Succeeded);
    let snapshot = store.snapshot().unwrap();
    assert_eq!(snapshot.runs[0].state, RunState::Succeeded);
    assert_eq!(snapshot.completions[0].summary, "Implemented and verified");
    assert_eq!(snapshot.completions[0].candidate_id, Some(candidate));
    assert!(
        snapshot
            .sessions
            .iter()
            .filter(|session| session.binding.role() == SessionRole::Worker)
            .all(|session| session.state == ManagedSessionState::Running)
    );
}

#[tokio::test]
async fn final_completion_refuses_changed_files_and_cannot_succeed_after_they_are_restored() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("output.txt"), "Result").unwrap();
    let mut store = Store::open(&dir.path().join("control/state.sqlite3")).unwrap();
    let (owner, run, candidate, original) = verified_candidate(&mut store, dir.path()).await;
    let revision = store.snapshot().unwrap().runs[0].plan_revision;
    let ticket = store
        .request_change_completion(
            RequestId::new(),
            owner,
            run,
            revision,
            candidate,
            "Done".into(),
        )
        .unwrap()
        .value;
    let mut changed = original.clone();
    changed.files.fingerprint = "sha256:changed".into();
    assert_eq!(
        store
            .record_change_completion(RequestId::new(), ticket.id, Ok(changed))
            .unwrap()
            .value
            .state,
        CompletionRequestState::Rejected
    );
    assert_eq!(
        store
            .record_change_completion(RequestId::new(), ticket.id, Ok(original))
            .unwrap()
            .value
            .state,
        CompletionRequestState::Rejected
    );
    let snapshot = store.snapshot().unwrap();
    assert!(snapshot.completions.is_empty());
    assert_eq!(snapshot.candidates[0].state, CandidateState::Stale);
}

#[tokio::test]
async fn a_plan_update_while_final_confirmation_runs_keeps_the_new_request_active() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("output.txt"), "Result").unwrap();
    let mut store = Store::open(&dir.path().join("control/state.sqlite3")).unwrap();
    let (owner, run, candidate, proof) = verified_candidate(&mut store, dir.path()).await;
    let current = store.snapshot().unwrap().runs[0].clone();
    let ticket = store
        .request_change_completion(
            RequestId::new(),
            owner,
            run,
            current.plan_revision,
            candidate,
            "Done".into(),
        )
        .unwrap()
        .value;
    let mut brief = current.brief;
    brief.constraints.push("Include another result".into());
    store
        .update_run(
            RequestId::new(),
            owner,
            run,
            current.plan_revision,
            brief,
            vec![],
        )
        .unwrap();
    assert_eq!(
        store
            .record_change_completion(RequestId::new(), ticket.id, Ok(proof))
            .unwrap()
            .value
            .state,
        CompletionRequestState::Rejected
    );
    assert!(store.snapshot().unwrap().completions.is_empty());
    assert_eq!(store.snapshot().unwrap().runs[0].state, RunState::Active);
}

#[tokio::test]
async fn daemon_final_confirmation_completes_a_scoped_request_without_a_window_or_model_turn() {
    use adroit::control::{
        capture::CaptureSupervisor, client::Client, protocol::Command, service::Service,
    };
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("output.txt"), "Result").unwrap();
    let root = dir.path().join("control");
    let mut store = Store::open(&root.join("state.sqlite3")).unwrap();
    let (owner, run, candidate, _) = verified_candidate(&mut store, dir.path()).await;
    let revision = store.snapshot().unwrap().runs[0].plan_revision;
    let client = Client::session(&root, store.grant(owner).unwrap());
    drop(store);
    let service = Service::bind(&root).await.unwrap();
    let response = client
        .request(
            RequestId::new(),
            Command::RequestChangeCompletion {
                run_id: run,
                expected_revision: revision,
                candidate_id: candidate,
                summary: "Verified change".into(),
            },
        )
        .await
        .unwrap();
    assert_eq!(response["value"]["state"], "pending");
    let mut capture = CaptureSupervisor::new(service.actor());
    tokio::time::timeout(std::time::Duration::from_secs(3), async {
        loop {
            capture.tick().await.unwrap();
            let snapshot = client
                .request(RequestId::new(), Command::Snapshot)
                .await
                .unwrap();
            if snapshot["completion_requests"][0]["state"] == "succeeded" {
                assert_eq!(snapshot["runs"][0]["state"], "succeeded");
                assert_eq!(
                    snapshot["completions"][0]["candidate_id"],
                    candidate.to_string()
                );
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        }
    })
    .await
    .unwrap();
}

#[tokio::test]
async fn cancellation_during_final_confirmation_cannot_reactivate_or_complete_the_run() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("output.txt"), "Result").unwrap();
    let mut store = Store::open(&dir.path().join("control/state.sqlite3")).unwrap();
    let (owner, run, candidate, proof) = verified_candidate(&mut store, dir.path()).await;
    let revision = store.snapshot().unwrap().runs[0].plan_revision;
    let ticket = store
        .request_change_completion(
            RequestId::new(),
            owner,
            run,
            revision,
            candidate,
            "Done".into(),
        )
        .unwrap()
        .value;
    store.cancel_run(RequestId::new(), owner, run).unwrap();
    assert_eq!(
        store
            .record_change_completion(RequestId::new(), ticket.id, Ok(proof))
            .unwrap()
            .value
            .state,
        CompletionRequestState::Rejected
    );
    assert_eq!(store.snapshot().unwrap().runs[0].state, RunState::Cancelled);
    assert!(store.snapshot().unwrap().completions.is_empty());
}

#[tokio::test]
async fn a_new_blocking_question_after_completion_was_requested_still_prevents_success() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("output.txt"), "Result").unwrap();
    let mut store = Store::open(&dir.path().join("control/state.sqlite3")).unwrap();
    let (owner, run, candidate, proof) = verified_candidate(&mut store, dir.path()).await;
    let snapshot = store.snapshot().unwrap();
    let revision = snapshot.runs[0].plan_revision;
    let attempt = snapshot
        .attempts
        .iter()
        .find(|attempt| attempt.candidate_input.is_some())
        .unwrap();
    let ticket = store
        .request_change_completion(
            RequestId::new(),
            owner,
            run,
            revision,
            candidate,
            "Done".into(),
        )
        .unwrap()
        .value;
    store
        .send_message(
            RequestId::new(),
            SessionRef {
                id: attempt.session_id,
                generation: 1,
            },
            AgentMessage {
                id: MessageId::new(),
                run_id: run,
                from: attempt.session_id,
                to: owner.id,
                task_id: Some(attempt.task_id),
                attempt_id: Some(attempt.id),
                revision: Some(attempt.task_revision),
                kind: MessageKind::Question,
                body: "A final decision is still required".into(),
                reply_to: None,
                blocks_work: true,
            },
        )
        .unwrap();
    assert_eq!(
        store
            .record_change_completion(RequestId::new(), ticket.id, Ok(proof))
            .unwrap()
            .value
            .state,
        CompletionRequestState::Rejected
    );
    assert!(store.snapshot().unwrap().completions.is_empty());
    assert_eq!(store.snapshot().unwrap().runs[0].state, RunState::Active);
}
