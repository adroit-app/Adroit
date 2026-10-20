//! Worker claims become quality evidence only after the input check finishes.
use super::*;
use std::collections::HashSet;

pub(super) fn validate_gate(gate: &QualityGate) -> Result<()> {
    if gate.commands.len() > 32
        || encode(gate)?.len() > 32 * 1024
        || gate.commands.iter().any(|command| {
            command.is_empty()
                || command[0].trim().is_empty()
                || command.len() > 128
                || command.iter().any(|arg| arg.contains('\0'))
        })
    {
        return Err(invalid(
            "a quality gate needs bounded valid command argument arrays",
        ));
    }
    Ok(())
}

pub(super) fn current_candidate(
    connection: &Connection,
    task: &TaskRecord,
    attempt: &AttemptRecord,
) -> Result<bool> {
    let Some(id) = task.brief.candidate_id else {
        return Ok(true);
    };
    let candidate: CandidateRecord = require(connection, Table::Candidates, id)?;
    let run: RunRecord = require(connection, Table::Runs, task.run_id)?;
    Ok(run.candidate_id == Some(id)
        && matches!(
            candidate.state,
            CandidateState::Prepared | CandidateState::Verified
        )
        && attempt.candidate_input.is_some()
        && attempt.candidate_input == candidate.proof.as_ref().map(CandidateProof::summary)
        && super::candidates::source(connection, &candidate.plan).is_ok())
}

pub(super) fn validate_report(
    connection: &Connection,
    task: &TaskRecord,
    attempt: &AttemptRecord,
    result: &TaskResult,
) -> Result<()> {
    if result.status != ReportStatus::Succeeded {
        return Ok(());
    }
    if task.brief.purpose == TaskPurpose::Review
        && !matches!(result.artifact, Some(TaskArtifact::Review { .. }))
    {
        return Err(invalid(
            "a successful review needs an explicit review decision",
        ));
    }
    if let Some(TaskArtifact::Review {
        reviewed_revision, ..
    }) = &result.artifact
    {
        let area = attempt
            .work_area_id
            .map(|id| require::<WorkArea>(connection, Table::WorkAreas, id))
            .transpose()?;
        if reviewed_revision.as_ref().is_some_and(|id| {
            !matches!(id.len(), 40 | 64) || !id.bytes().all(|byte| byte.is_ascii_hexdigit())
        }) || (area.is_some_and(|area| area.backend != WorkAreaBackend::Directory)
            && reviewed_revision.is_none())
        {
            return Err(invalid(
                "a native review needs its full immutable reviewed revision",
            ));
        }
    }
    if let Some(id) = task.brief.candidate_id {
        if attempt.candidate_input.is_none() {
            return Err(conflict("the check has no confirmed candidate input"));
        }
        if result
            .checks
            .iter()
            .any(|check| check.exit_code != 0 || check.candidate != id.to_string())
        {
            return Err(invalid(
                "a successful candidate check needs passing commands for that candidate",
            ));
        }
        if task.brief.purpose == TaskPurpose::Verify && result.checks.is_empty() {
            return Err(invalid(
                "verification success needs its actual check commands",
            ));
        }
        let candidate: CandidateRecord = require(connection, Table::Candidates, id)?;
        if let Some(TaskArtifact::Review {
            reviewed_revision, ..
        }) = &result.artifact
            && candidate.proof.as_ref().map(|proof| &proof.revision) != Some(reviewed_revision)
        {
            return Err(invalid("the review names a different candidate revision"));
        }
    }
    Ok(())
}

fn save(connection: &Connection, record: &CandidateCheckRecord) -> Result<()> {
    connection.execute("INSERT INTO candidate_checks(id,candidate_id,task_id,report_id,state,data) VALUES (?1,?2,?3,?4,?5,?6) ON CONFLICT(id) DO UPDATE SET state=excluded.state,data=excluded.data",params![record.attempt_id.to_string(),record.candidate_id.to_string(),record.task.task_id.to_string(),record.report_id.to_string(),record.state.word(),encode(record)?])?;
    Ok(())
}

pub(super) fn queue(connection: &Connection, task: &TaskRecord, report: &TaskReport) -> Result<()> {
    let Some(candidate_id) = task.brief.candidate_id else {
        return Ok(());
    };
    if task.state != TaskState::Verifying {
        return Ok(());
    }
    let record = CandidateCheckRecord {
        attempt_id: report.attempt_id,
        candidate_id,
        task: report.task,
        report_id: report.message_id,
        state: CandidateCheckState::Pending,
        observed: None,
        error: None,
    };
    save(connection, &record)?;
    event(
        connection,
        Some(task.run_id),
        "candidate_check_pending",
        &record,
    )
}

pub(super) fn refresh(connection: &Connection, id: CandidateId) -> Result<()> {
    let mut candidate: CandidateRecord = require(connection, Table::Candidates, id)?;
    if !matches!(
        candidate.state,
        CandidateState::Prepared | CandidateState::Verified
    ) {
        return Ok(());
    }
    let run: RunRecord = require(connection, Table::Runs, candidate.plan.run_id)?;
    if run.candidate_id != Some(id)
        || super::candidates::source(connection, &candidate.plan).is_err()
    {
        return Ok(());
    }
    let expected = candidate.proof.as_ref().map(CandidateProof::summary);
    let mut passed = HashSet::new();
    let mut approved = HashSet::new();
    let mut statement = connection.prepare(
        "SELECT data FROM candidate_checks WHERE candidate_id = ?1 AND state = 'validated'",
    )?;
    let records = statement
        .query_map([id.to_string()], |row| row.get::<_, String>(0))?
        .map(|row| decode(&row?))
        .collect::<Result<Vec<CandidateCheckRecord>>>()?;
    for record in records {
        let task: TaskRecord = require(connection, Table::Tasks, record.task.task_id)?;
        let attempt: AttemptRecord = require(connection, Table::Attempts, record.attempt_id)?;
        if task.state != TaskState::Succeeded
            || task.revision != record.task.revision
            || task.current_attempt != Some(record.attempt_id)
            || task.brief.candidate_id != Some(id)
            || attempt.candidate_input != expected
            || !super::dependencies::current_inputs(connection, &task, &attempt)?
        {
            continue;
        }
        let report: TaskReportRecord = require(connection, Table::Reports, record.report_id)?;
        if !report.applied || report.report.result.status != ReportStatus::Succeeded {
            continue;
        }
        if task.brief.purpose == TaskPurpose::Verify {
            for check in &report.report.result.checks {
                if check.exit_code == 0 && check.candidate == id.to_string() {
                    passed.insert(check.command.clone());
                }
            }
        }
        if task.brief.purpose == TaskPurpose::Review
            && matches!(
                report.report.result.artifact,
                Some(TaskArtifact::Review {
                    decision: ReviewDecision::Approved,
                    ..
                })
            )
        {
            approved.insert(task.id);
        }
    }
    let reviews: Vec<_> = all::<TaskRecord>(connection, Table::Tasks)?
        .into_iter()
        .filter(|task| {
            task.run_id == run.id
                && task.brief.candidate_id == Some(id)
                && task.brief.purpose == TaskPurpose::Review
                && task.brief.required
                && task.state != TaskState::Cancelled
        })
        .collect();
    let gate = &run.brief.quality_gate;
    let declared = !gate.commands.is_empty() || gate.review_required || !reviews.is_empty();
    let verified = declared
        && gate.commands.iter().all(|command| passed.contains(command))
        && reviews.iter().all(|task| approved.contains(&task.id))
        && (!gate.review_required || !approved.is_empty());
    let state = if verified {
        CandidateState::Verified
    } else {
        CandidateState::Prepared
    };
    if candidate.state != state {
        candidate.state = state;
        super::candidates::save(connection, &candidate)?;
        event(
            connection,
            Some(run.id),
            "candidate_quality_changed",
            &candidate.summary(),
        )?;
    }
    Ok(())
}

pub(super) fn refresh_tasks(connection: &Connection, tasks: &HashSet<TaskId>) -> Result<()> {
    let mut candidates = HashSet::new();
    let mut statement = connection
        .prepare("SELECT DISTINCT candidate_id FROM candidate_checks WHERE task_id = ?1")?;
    for task in tasks {
        for id in statement.query_map([task.to_string()], |row| row.get::<_, String>(0))? {
            candidates.insert(
                id?.parse::<CandidateId>()
                    .map_err(|_| invalid("the stored check candidate is invalid"))?,
            );
        }
    }
    for id in candidates {
        refresh(connection, id)?;
    }
    Ok(())
}

impl Store {
    pub fn pending_candidate_checks(&self, limit: u32) -> Result<Vec<CandidateCheckRecord>> {
        let mut statement = self.connection.prepare(
            "SELECT data FROM candidate_checks WHERE state = 'pending' ORDER BY rowid LIMIT ?1",
        )?;
        statement
            .query_map([limit.min(64)], |row| row.get::<_, String>(0))?
            .map(|row| decode(&row?))
            .collect()
    }

    pub fn record_candidate_check(
        &mut self,
        request: RequestId,
        attempt_id: AttemptId,
        outcome: std::result::Result<CandidateProof, String>,
    ) -> Result<Reply<CandidateCheckRecord>> {
        self.transact(request,json!({"op":"record_candidate_check","attempt_id":attempt_id,"outcome":outcome}),|connection| {
            let mut check: CandidateCheckRecord = require(connection,Table::CandidateChecks,attempt_id)?;
            if check.state != CandidateCheckState::Pending { return Ok(check); }
            let mut task: TaskRecord = require(connection,Table::Tasks,check.task.task_id)?;
            let attempt: AttemptRecord = require(connection,Table::Attempts,attempt_id)?;
            let mut candidate: CandidateRecord = require(connection,Table::Candidates,check.candidate_id)?;
            let run: RunRecord = require(connection,Table::Runs,task.run_id)?;
            if task.revision != check.task.revision || task.current_attempt != Some(attempt_id) || task.brief.candidate_id != Some(check.candidate_id) || task.state != TaskState::Verifying {
                check.state = CandidateCheckState::Obsolete;
                check.error = Some("The check assignment changed before confirmation".into());
            } else if run.candidate_id != Some(check.candidate_id) || !matches!(run.state,RunState::Active|RunState::Verifying) {
                check.state = CandidateCheckState::Obsolete;
                check.error = Some("The run selected another candidate or stopped".into());
                task.state = TaskState::Cancelled;
                save_task(connection,&task)?;
            } else {
                let observed = outcome.as_ref().ok().map(CandidateProof::summary);
                let valid = observed.is_some() && observed == attempt.candidate_input && observed == candidate.proof.as_ref().map(CandidateProof::summary) && matches!(candidate.state,CandidateState::Prepared|CandidateState::Verified) && super::dependencies::current_inputs(connection,&task,&attempt)? && super::candidates::source(connection,&candidate.plan).is_ok();
                check.observed = observed;
                if valid {
                    check.state = CandidateCheckState::Validated;
                    task.state = TaskState::Succeeded;
                } else {
                    check.state = CandidateCheckState::Rejected;
                    check.error = Some(outcome.err().unwrap_or_else(||"Candidate inputs changed during verification".into()));
                    task.state = TaskState::Failed;
                    candidate.state = CandidateState::Stale;
                    candidate.error = check.error.clone();
                    super::candidates::save(connection,&candidate)?;
                    event(connection,Some(run.id),"candidate_inputs_changed",&candidate.summary())?;
                    super::dependencies::invalidate_dependents(connection,task.id)?;
                }
                save_task(connection,&task)?;
                event(connection,Some(run.id),"candidate_check_task_confirmed",&task)?;
            }
            save(connection,&check)?;
            refresh(connection,check.candidate_id)?;
            let current: CandidateRecord = require(connection,Table::Candidates,check.candidate_id)?;
            event(connection,Some(run.id),"candidate_check_finished",&check)?;
            store_message(connection,&AgentMessage {id:MessageId::new(),run_id:run.id,from:run.leader_id,to:run.leader_id,task_id:None,attempt_id:None,revision:None,kind:MessageKind::Note,body:encode(&json!({"type":"candidate_check_finished","actor":"adroit_controller","check":check,"candidate_state":current.state}))?,reply_to:None,blocks_work:false})?;
            Ok(check)
        })
    }
}
