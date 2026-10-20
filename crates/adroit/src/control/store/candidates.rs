//! Immutable candidate sources and asynchronous capture outcomes.
use super::*;
use std::collections::HashSet;

fn save(connection: &Connection, candidate: &CandidateRecord) -> Result<()> {
    connection.execute("INSERT INTO candidates(id,run_id,source_task_id,data) VALUES (?1,?2,?3,?4) ON CONFLICT(id) DO UPDATE SET data=excluded.data",params![candidate.plan.id.to_string(),candidate.plan.run_id.to_string(),candidate.plan.source.task_id.to_string(),encode(candidate)?])?;
    Ok(())
}

fn source(connection: &Connection, plan: &CandidatePlan) -> Result<TaskReportRecord> {
    let task: TaskRecord = require(connection, Table::Tasks, plan.source.task_id)?;
    if task.run_id != plan.run_id {
        return Err(ControlError::new(
            ErrorCode::OwnershipMismatch,
            "the candidate source belongs to another run",
        ));
    }
    if task.revision != plan.source.revision
        || task.current_attempt != Some(plan.attempt_id)
        || task.state != TaskState::Succeeded
    {
        return Err(conflict(
            "the candidate needs the current successful source task",
        ));
    }
    let attempt: AttemptRecord = require(connection, Table::Attempts, plan.attempt_id)?;
    if attempt.task_id != task.id
        || attempt.task_revision != task.revision
        || attempt.state != AttemptState::Succeeded
        || !super::dependencies::current_inputs(connection, &task, &attempt)?
    {
        return Err(conflict("the candidate source inputs are outdated"));
    }
    let data: Option<String> = connection
        .query_row(
            "SELECT data FROM task_reports WHERE attempt_id = ?1",
            [attempt.id.to_string()],
            |row| row.get(0),
        )
        .optional()?;
    let report: TaskReportRecord = data
        .as_deref()
        .map(decode)
        .transpose()?
        .ok_or_else(|| conflict("the candidate source has no applied success report"))?;
    if !report.applied
        || report.report.task != plan.source
        || report.report.result.status != ReportStatus::Succeeded
    {
        return Err(conflict(
            "the candidate source has no applied success report",
        ));
    }
    if !matches!(
        report.report.result.artifact,
        Some(TaskArtifact::Change { .. })
    ) {
        return Err(invalid("a completion candidate needs a change artifact"));
    }
    Ok(report)
}

fn invalidate(
    connection: &Connection,
    mut candidates: Vec<CandidateRecord>,
    reason: &str,
) -> Result<()> {
    for candidate in &mut candidates {
        candidate.state = CandidateState::Stale;
        candidate.error = Some(reason.into());
        save(connection, candidate)?;
        event(
            connection,
            Some(candidate.plan.run_id),
            "candidate_invalidated",
            &candidate.summary(),
        )?;
    }
    Ok(())
}

pub(super) fn invalidate_sources(connection: &Connection, tasks: &HashSet<TaskId>) -> Result<()> {
    let mut candidates = Vec::new();
    let mut statement = connection.prepare("SELECT data FROM candidates WHERE source_task_id = ?1 AND json_extract(data,'$.state') IN ('requested','prepared','verified') ORDER BY rowid")?;
    for task in tasks {
        candidates.extend(
            statement
                .query_map([task.to_string()], |row| row.get::<_, String>(0))?
                .map(|row| decode(&row?))
                .collect::<Result<Vec<CandidateRecord>>>()?,
        );
    }
    invalidate(
        connection,
        candidates,
        "The candidate source was revised or cancelled",
    )
}

pub(super) fn invalidate_run(connection: &Connection, run: RunId) -> Result<()> {
    let mut statement = connection.prepare("SELECT data FROM candidates WHERE run_id = ?1 AND json_extract(data,'$.state') IN ('requested','prepared','verified') ORDER BY rowid")?;
    let candidates = statement
        .query_map([run.to_string()], |row| row.get::<_, String>(0))?
        .map(|row| decode(&row?))
        .collect::<Result<Vec<CandidateRecord>>>()?;
    invalidate(connection, candidates, "The candidate run was cancelled")
}

fn notice(connection: &Connection, candidate: &CandidateRecord) -> Result<()> {
    let run: RunRecord = require(connection, Table::Runs, candidate.plan.run_id)?;
    store_message(
        connection,
        &AgentMessage {
            id: MessageId::new(),
            run_id: run.id,
            from: run.leader_id,
            to: run.leader_id,
            task_id: None,
            attempt_id: None,
            revision: None,
            kind: MessageKind::Note,
            body: encode(
                &json!({"type":"candidate_capture_finished","actor":"adroit_controller","candidate_id":candidate.plan.id,"state":candidate.state,"fingerprint":candidate.proof.as_ref().map(|proof|&proof.files.fingerprint),"error":candidate.error}),
            )?,
            reply_to: None,
            blocks_work: false,
        },
    )
}

impl Store {
    pub fn propose_candidate(
        &mut self,
        request: RequestId,
        identity: SessionRef,
        expected_revision: u64,
        plan: CandidatePlan,
    ) -> Result<Reply<CandidateRecord>> {
        self.transact(request,json!({"op":"propose_candidate","caller":identity,"expected_revision":expected_revision,"plan":plan}),|connection| {
            let mut run = owner(connection,identity,plan.run_id)?;
            if let Some(existing) = load::<CandidateRecord>(connection,Table::Candidates,plan.id)? {
                if existing.plan != plan { return Err(conflict("the candidate ID was reused with different content")); }
                return Ok(existing);
            }
            if run.plan_revision != expected_revision { return Err(ControlError::new(ErrorCode::RevisionConflict,"the plan revision is outdated")); }
            if encode(&plan)?.len() > 64*1024 { return Err(invalid("a candidate plan exceeds 64 KiB")); }
            if run.brief.quality_gate.commands.is_empty() && !run.brief.quality_gate.review_required { return Err(invalid("declare a quality command or required review before proposing a change candidate")); }
            for command in &run.brief.quality_gate.commands {
                if command.is_empty() || command[0].trim().is_empty() || command.iter().any(|arg|arg.contains('\0')) { return Err(invalid("a quality gate needs valid command argument arrays")); }
            }
            let report = source(connection,&plan)?;
            if let Some(previous) = run.candidate_id {
                let mut previous: CandidateRecord = require(connection,Table::Candidates,previous)?;
                previous.state = CandidateState::Superseded;
                save(connection,&previous)?;
                event(connection,Some(run.id),"candidate_superseded",&previous.summary())?;
            }
            let candidate = CandidateRecord {source_report:report.report.message_id,artifact:report.report.result.artifact.expect("validated change artifact"),plan,state:CandidateState::Requested,proof:None,error:None};
            save(connection,&candidate)?;
            run.candidate_id = Some(candidate.plan.id);
            run.brief.completion_kind = CompletionKind::Change;
            run.plan_revision = run.plan_revision.checked_add(1).ok_or_else(||conflict("the plan revision is exhausted"))?;
            run.state = RunState::Active;
            save_run(connection,&run)?;
            event(connection,Some(run.id),"candidate_requested",&candidate.summary())?;
            Ok(candidate)
        })
    }

    pub fn record_candidate_capture(
        &mut self,
        request: RequestId,
        id: CandidateId,
        outcome: std::result::Result<CandidateProof, String>,
    ) -> Result<Reply<CandidateRecord>> {
        self.transact(
            request,
            json!({"op":"record_candidate_capture","candidate_id":id,"outcome":outcome}),
            |connection| {
                let mut candidate: CandidateRecord = require(connection, Table::Candidates, id)?;
                if candidate.state != CandidateState::Requested {
                    return Ok(candidate);
                }
                let run: RunRecord = require(connection, Table::Runs, candidate.plan.run_id)?;
                if run.candidate_id != Some(id)
                    || !matches!(run.state, RunState::Active | RunState::Verifying)
                    || source(connection, &candidate.plan).is_err()
                {
                    candidate.state = CandidateState::Stale;
                    candidate.error =
                        Some("The run or candidate source changed during capture".into());
                } else {
                    match outcome {
                        Ok(proof) => {
                            let TaskArtifact::Change {
                                backend,
                                work_area_id,
                                revision,
                                ..
                            } = &candidate.artifact
                            else {
                                unreachable!("validated candidate source")
                            };
                            if proof.work_area_id != *work_area_id
                                || proof.backend != *backend
                                || proof.revision != *revision
                                || proof.files.version != 1
                                || !proof.files.fingerprint.starts_with("sha256:")
                            {
                                return Err(invalid(
                                    "the captured proof does not match the proposed source",
                                ));
                            }
                            candidate.proof = Some(proof);
                            candidate.state = CandidateState::Prepared;
                        }
                        Err(error) => {
                            candidate.error = Some(error);
                            candidate.state = CandidateState::Failed;
                        }
                    }
                }
                save(connection, &candidate)?;
                event(
                    connection,
                    Some(run.id),
                    "candidate_capture_finished",
                    &candidate.summary(),
                )?;
                notice(connection, &candidate)?;
                Ok(candidate)
            },
        )
    }
}

impl Store {
    pub fn candidate(&self, id: CandidateId) -> Result<CandidateRecord> {
        require(&self.connection, Table::Candidates, id)
    }

    pub fn pending_candidates(&self, limit: u32) -> Result<Vec<CandidateRecord>> {
        let mut statement = self.connection.prepare("SELECT data FROM candidates WHERE json_extract(data,'$.state') = 'requested' ORDER BY rowid LIMIT ?1")?;
        statement
            .query_map([limit.min(64)], |row| row.get::<_, String>(0))?
            .map(|row| decode(&row?))
            .collect()
    }

    pub fn work_area(&self, id: WorkAreaId) -> Result<WorkArea> {
        require(&self.connection, Table::WorkAreas, id)
    }
}

pub(super) fn task_candidate(
    connection: &Connection,
    run_id: RunId,
    brief: &TaskBrief,
    dependencies: &[TaskId],
    prepared: bool,
) -> Result<Option<CandidateRecord>> {
    let Some(id) = brief.candidate_id else {
        return Ok(None);
    };
    let candidate: CandidateRecord = require(connection, Table::Candidates, id)?;
    if candidate.plan.run_id != run_id {
        return Err(ControlError::new(
            ErrorCode::OwnershipMismatch,
            "the check candidate belongs to another run",
        ));
    }
    let run: RunRecord = require(connection, Table::Runs, run_id)?;
    if run.candidate_id != Some(id)
        || !matches!(
            candidate.state,
            CandidateState::Requested | CandidateState::Prepared | CandidateState::Verified
        )
    {
        return Err(conflict("the check needs the selected current candidate"));
    }
    if !matches!(brief.purpose, TaskPurpose::Review | TaskPurpose::Verify) {
        return Err(invalid(
            "candidate inputs belong to review or verification tasks",
        ));
    }
    let TaskArtifact::Change { work_area_id, .. } = candidate.artifact else {
        return Err(invalid("the candidate lost its change artifact"));
    };
    if brief.workdir != WorkdirPolicy::Existing(work_area_id)
        || !dependencies.contains(&candidate.plan.source.task_id)
    {
        return Err(invalid(
            "a candidate check needs its existing working area and source dependency",
        ));
    }
    source(connection, &candidate.plan)?;
    if prepared
        && (candidate.proof.is_none()
            || !matches!(
                candidate.state,
                CandidateState::Prepared | CandidateState::Verified
            ))
    {
        return Err(conflict("the check candidate is not prepared"));
    }
    Ok(Some(candidate))
}

impl Store {
    pub fn record_candidate_input(
        &mut self,
        request: RequestId,
        identity: SessionRef,
        wanted: TaskVersion,
        attempt_id: AttemptId,
        outcome: std::result::Result<CandidateProof, String>,
    ) -> Result<Reply<Option<AttemptRecord>>> {
        self.transact(request,json!({"op":"record_candidate_input","caller":identity,"task":wanted,"attempt_id":attempt_id,"outcome":outcome}),|connection| {
            let task = version(connection,identity,wanted)?;
            let mut attempt: AttemptRecord = require(connection,Table::Attempts,attempt_id)?;
            if task.current_attempt != Some(attempt_id) || attempt.task_id != task.id || attempt.task_revision != task.revision || attempt.state != AttemptState::Preparing { return Err(conflict("the check attempt no longer matches preparation")); }
            let mut candidate = task_candidate(connection,task.run_id,&task.brief,&task.dependencies,true)?.ok_or_else(||invalid("the task has no candidate inputs"))?;
            let expected = candidate.proof.as_ref().expect("prepared proof").summary();
            if attempt.work_area_id != Some(expected.work_area_id) { return Err(invalid("the check attempt has a different working area")); }
            let observation = match outcome {
                Ok(proof) if proof.files.version == 1 && proof.summary() == expected => Some(proof.summary()),
                Ok(_) => {candidate.error = Some("The candidate inputs changed before check startup".into());None},
                Err(error) => {candidate.error = Some(format!("Cannot confirm candidate inputs before check startup: {error}"));None},
            };
            if let Some(observation) = observation {
                attempt.candidate_input = Some(observation);
                save_attempt(connection,&attempt)?;
                event(connection,Some(task.run_id),"candidate_input_recorded",&attempt)?;
                Ok(Some(attempt))
            } else {
                candidate.state = CandidateState::Stale;
                save(connection,&candidate)?;
                event(connection,Some(task.run_id),"candidate_inputs_changed",&candidate.summary())?;
                notice(connection,&candidate)?;
                Ok(None)
            }
        })
    }
}
