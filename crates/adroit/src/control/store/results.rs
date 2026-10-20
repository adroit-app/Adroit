//! Explicit outcomes and cooperative stopping; terminal lifetime stays independent.
use super::*;

pub(super) fn request_stop(
    connection: &Connection,
    task: &TaskRecord,
    attempt: &mut AttemptRecord,
    reason: &str,
) -> Result<()> {
    if attempt.state == AttemptState::CancelRequested {
        return Ok(());
    }
    if !attempt.state.holds_slot() {
        return Err(conflict("the attempt has already finished"));
    }
    attempt.state = AttemptState::CancelRequested;
    save_attempt(connection, attempt)?;
    let run: RunRecord = require(connection, Table::Runs, task.run_id)?;
    store_message(
        connection,
        &AgentMessage {
            id: MessageId::new(),
            run_id: task.run_id,
            from: run.leader_id,
            to: attempt.session_id,
            task_id: Some(task.id),
            attempt_id: Some(attempt.id),
            revision: Some(attempt.task_revision),
            kind: MessageKind::Cancel,
            body: reason.into(),
            reply_to: None,
            blocks_work: false,
        },
    )?;
    event(
        connection,
        Some(task.run_id),
        "attempt_cancel_requested",
        attempt,
    )
}

fn validate_result(
    connection: &Connection,
    attempt: &AttemptRecord,
    result: &TaskResult,
) -> Result<()> {
    if result.summary.trim().is_empty() {
        return Err(invalid("a task result needs a summary"));
    }
    if encode(result)?.len() > 64 * 1024 {
        return Err(invalid(
            "a task result exceeds 64 KiB; reference files for large artifacts",
        ));
    }
    if result.status == ReportStatus::Succeeded && result.artifact.is_none() {
        return Err(invalid("a successful result needs an artifact"));
    }
    if result.status == ReportStatus::Succeeded && !result.unresolved.is_empty() {
        return Err(invalid("a successful result cannot leave unresolved work"));
    }
    for check in &result.checks {
        if check.command.is_empty()
            || check.command[0].trim().is_empty()
            || check.candidate.trim().is_empty()
        {
            return Err(invalid(
                "a check needs a command and its candidate identity",
            ));
        }
    }
    if let Some(TaskArtifact::Change {
        backend,
        work_area_id,
        revision,
        manifest,
        candidate_fingerprint,
    }) = &result.artifact
    {
        if attempt.work_area_id != Some(*work_area_id) {
            return Err(invalid("the artifact belongs to a different working area"));
        }
        let area: WorkArea = require(connection, Table::WorkAreas, work_area_id)?;
        if area.backend != *backend {
            return Err(invalid(
                "the artifact backend does not match its working area",
            ));
        }
        match backend {
            WorkAreaBackend::Directory => {
                if manifest.is_empty()
                    || candidate_fingerprint
                        .as_ref()
                        .is_none_or(|id| id.trim().is_empty())
                {
                    return Err(invalid(
                        "a directory artifact needs a manifest and candidate fingerprint",
                    ));
                }
            }
            WorkAreaBackend::Jj | WorkAreaBackend::Git => {
                if revision.as_ref().is_none_or(|id| {
                    !matches!(id.len(), 40 | 64) || !id.bytes().all(|byte| byte.is_ascii_hexdigit())
                }) {
                    return Err(invalid(
                        "a VCS artifact needs a complete immutable commit ID",
                    ));
                }
            }
        }
    }
    Ok(())
}

impl Store {
    pub fn cancel_task(
        &mut self,
        request: RequestId,
        identity: SessionRef,
        wanted: TaskVersion,
    ) -> Result<Reply<TaskRecord>> {
        self.transact(
            request,
            json!({"op":"cancel_task", "caller":identity, "task":wanted}),
            |connection| {
                let mut task = version(connection, identity, wanted)?;
                if let Some(id) = task.current_attempt {
                    let mut attempt: AttemptRecord = require(connection, Table::Attempts, id)?;
                    if attempt.state.holds_slot() {
                        request_stop(
                            connection,
                            &task,
                            &mut attempt,
                            "The Leader requested cancellation; stop work and report cancellation",
                        )?;
                        return Ok(task);
                    }
                }
                if task.state != TaskState::Cancelled {
                    task.state = TaskState::Cancelled;
                    save_task(connection, &task)?;
                    super::dependencies::invalidate_dependents(connection, task.id)?;
                    event(connection, Some(task.run_id), "task_cancelled", &task)?;
                }
                Ok(task)
            },
        )
    }

    pub fn report_task(
        &mut self,
        request: RequestId,
        identity: SessionRef,
        report: TaskReport,
    ) -> Result<Reply<TaskReportRecord>> {
        self.transact(
            request,
            json!({"op":"report_task", "caller":identity, "report":report}),
            |connection| {
                let session = reference(connection, identity)?;
                let mut attempt: AttemptRecord =
                    require(connection, Table::Attempts, report.attempt_id)?;
                if session.binding.work() != Some((report.task.task_id, report.attempt_id))
                    || attempt.session_id != identity.id
                    || attempt.task_id != report.task.task_id
                    || attempt.task_revision != report.task.revision
                {
                    return Err(ControlError::new(
                        ErrorCode::OwnershipMismatch,
                        "the report does not match this worker's assignment",
                    ));
                }
                if let Some(existing) =
                    load::<TaskReportRecord>(connection, Table::Reports, report.message_id)?
                {
                    if existing.report != report {
                        return Err(conflict("the report ID was reused with different content"));
                    }
                    return Ok(existing);
                }
                let existing: bool = connection.query_row(
                    "SELECT EXISTS(SELECT 1 FROM task_reports WHERE attempt_id = ?1)",
                    [report.attempt_id.to_string()],
                    |row| row.get(0),
                )?;
                if existing {
                    return Err(conflict("this attempt already has a final result"));
                }
                if !matches!(
                    attempt.state,
                    AttemptState::Running | AttemptState::CancelRequested | AttemptState::Uncertain
                ) {
                    return Err(conflict(
                        "the attempt cannot report a result in its current state",
                    ));
                }
                validate_result(connection, &attempt, &report.result)?;
                let mut task: TaskRecord = require(connection, Table::Tasks, report.task.task_id)?;
                let mut run: RunRecord = require(connection, Table::Runs, task.run_id)?;
                let assignment_current = task.revision == attempt.task_revision
                    && task.current_attempt == Some(attempt.id);
                let active_run = matches!(run.state, RunState::Active | RunState::Verifying);
                let inputs_current =
                    super::dependencies::current_inputs(connection, &task, &attempt)?;
                let current = assignment_current && active_run && inputs_current;
                let invalidate_inputs = assignment_current && active_run && !inputs_current;
                let cancelled = attempt.state == AttemptState::CancelRequested;
                let applied =
                    current && (!cancelled || report.result.status == ReportStatus::Cancelled);
                attempt.state = if cancelled {
                    AttemptState::Cancelled
                } else {
                    match report.result.status {
                        ReportStatus::Succeeded => AttemptState::Succeeded,
                        ReportStatus::Failed | ReportStatus::Blocked => AttemptState::Failed,
                        ReportStatus::Cancelled => AttemptState::Cancelled,
                    }
                };
                save_attempt(connection, &attempt)?;
                if current {
                    task.state = if cancelled {
                        TaskState::Cancelled
                    } else {
                        match report.result.status {
                            ReportStatus::Succeeded => TaskState::Succeeded,
                            ReportStatus::Failed => TaskState::Failed,
                            ReportStatus::Blocked => TaskState::Blocked,
                            ReportStatus::Cancelled => TaskState::Cancelled,
                        }
                    };
                    save_task(connection, &task)?;
                    event(connection, Some(task.run_id), "task_result_applied", &task)?;
                } else if task.current_attempt == Some(attempt.id) {
                    task.current_attempt = None;
                    if invalidate_inputs {
                        task.state = TaskState::Planned;
                        task.revision = task
                            .revision
                            .checked_add(1)
                            .ok_or_else(|| conflict("the task revision is exhausted"))?;
                        run.plan_revision = run
                            .plan_revision
                            .checked_add(1)
                            .ok_or_else(|| conflict("the plan revision is exhausted"))?;
                        save_run(connection, &run)?;
                        event(
                            connection,
                            Some(task.run_id),
                            "task_inputs_invalidated",
                            &task,
                        )?;
                    }
                    save_task(connection, &task)?;
                    if invalidate_inputs {
                        super::dependencies::invalidate_dependents(connection, task.id)?;
                    }
                }
                let record = TaskReportRecord {
                    report,
                    run_id: task.run_id,
                    applied,
                };
                store_message(
                    connection,
                    &AgentMessage {
                        id: record.report.message_id,
                        run_id: task.run_id,
                        from: identity.id,
                        to: session.binding.leader_id(),
                        task_id: Some(task.id),
                        attempt_id: Some(attempt.id),
                        revision: Some(attempt.task_revision),
                        kind: MessageKind::Report,
                        body: encode(&record)?,
                        reply_to: None,
                        blocks_work: false,
                    },
                )?;
                connection.execute(
                    "INSERT INTO task_reports(id, attempt_id, data) VALUES (?1, ?2, ?3)",
                    params![
                        record.report.message_id.to_string(),
                        attempt.id.to_string(),
                        encode(&record)?
                    ],
                )?;
                event(
                    connection,
                    Some(task.run_id),
                    "task_result_recorded",
                    &record,
                )?;
                Ok(record)
            },
        )
    }
}
