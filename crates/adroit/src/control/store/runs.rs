//! Explicit request updates and cancellation scoped to one Leader's run.
use super::*;
use std::collections::HashSet;

pub(super) fn settle_cancellation(connection: &Connection, run_id: RunId) -> Result<()> {
    let mut run: RunRecord = require(connection, Table::Runs, run_id)?;
    if run.state != RunState::Cancelling {
        return Ok(());
    }
    let active: bool = connection.query_row("SELECT EXISTS(SELECT 1 FROM attempts JOIN tasks ON tasks.id = attempts.task_id WHERE tasks.run_id = ?1 AND attempts.active = 1)",[run_id.to_string()],|row|row.get(0))?;
    if !active {
        run.state = RunState::Cancelled;
        save_run(connection, &run)?;
        event(connection, Some(run_id), "run_cancelled", &run)?;
    }
    Ok(())
}

impl Store {
    pub fn complete_run(
        &mut self,
        request: RequestId,
        identity: SessionRef,
        run_id: RunId,
        expected_revision: u64,
        summary: String,
    ) -> Result<Reply<RunCompletion>> {
        self.transact(request,json!({"op":"complete_run","caller":identity,"run_id":run_id,"expected_revision":expected_revision,"summary":summary}),|connection| {
            let mut run = owner(connection,identity,run_id)?;
            if run.plan_revision != expected_revision { return Err(ControlError::new(ErrorCode::RevisionConflict,"the plan revision is outdated")); }
            if summary.trim().is_empty() || summary.len() > 64 * 1024 { return Err(invalid("a completion summary must contain at most 64 KiB of nonempty text")); }
            if !run.brief.unresolved.is_empty() { return Err(conflict("the run has unresolved questions")); }
            if run.brief.completion_kind != CompletionKind::Report { return Err(conflict("a change run needs a verified candidate")); }
            let tasks: Vec<TaskRecord> = all::<TaskRecord>(connection,Table::Tasks)?.into_iter().filter(|task|task.run_id == run_id).collect();
            let attempts = all::<AttemptRecord>(connection,Table::Attempts)?;
            if attempts.iter().any(|attempt|attempt.state.holds_slot() && tasks.iter().any(|task|task.id == attempt.task_id)) { return Err(conflict("the run still has an active attempt")); }
            let outcomes = all::<TaskReportRecord>(connection,Table::Reports)?;
            let mut reports = Vec::new();
            for task in &tasks {
                let current = task.current_attempt.and_then(|id| attempts.iter().find(|attempt|attempt.id == id));
                let result = current.and_then(|attempt| outcomes.iter().find(|record|record.applied && record.report.attempt_id == attempt.id && record.report.task == TaskVersion {task_id:task.id,revision:task.revision}));
                if result.is_some_and(|record|matches!(record.report.result.artifact,Some(TaskArtifact::Change {..}))) { return Err(conflict("change evidence needs a verified candidate")); }
                if !task.brief.required { continue; }
                let attempt = current.ok_or_else(||conflict("a required task has no current attempt"))?;
                let record = result.ok_or_else(||conflict("a required task has no current report"))?;
                if task.state != TaskState::Succeeded || attempt.state != AttemptState::Succeeded || attempt.task_revision != task.revision || record.report.result.status != ReportStatus::Succeeded || !super::dependencies::current_inputs(connection,task,attempt)? {
                    return Err(conflict("a required task has not succeeded with current inputs"));
                }
                reports.push(record.report.message_id);
            }
            if reports.is_empty() { return Err(conflict("completion needs at least one required result")); }
            let messages = all::<AgentMessage>(connection,Table::Messages)?;
            for question in messages.iter().filter(|message| message.run_id == run_id && message.kind == MessageKind::Question && message.blocks_work) {
                if let Some(task_id) = question.task_id {
                    let Some(task) = tasks.iter().find(|task|task.id == task_id) else { continue; };
                    if task.state == TaskState::Cancelled || question.revision.is_some_and(|revision|revision != task.revision) || question.attempt_id.is_some_and(|id|Some(id) != task.current_attempt) { continue; }
                }
                if !messages.iter().any(|answer|answer.kind == MessageKind::Answer && answer.reply_to == Some(question.id) && answer.from == question.to && answer.to == question.from) { return Err(conflict("a current blocking question has no addressed answer")); }
            }
            for mut task in tasks.into_iter().filter(|task|!task.brief.required && matches!(task.state,TaskState::Planned|TaskState::Queued|TaskState::Blocked)) {
                task.state = TaskState::Cancelled;
                save_task(connection,&task)?;
                event(connection,Some(run_id),"optional_task_skipped",&task)?;
            }
            let completion = RunCompletion {run_id,plan_revision:run.plan_revision,summary,reports};
            connection.execute("INSERT INTO run_completions(id,data) VALUES (?1,?2)",params![run_id.to_string(),encode(&completion)?])?;
            run.state = RunState::Succeeded;
            save_run(connection,&run)?;
            event(connection,Some(run_id),"run_completed",&completion)?;
            Ok(completion)
        })
    }

    pub fn update_run(
        &mut self,
        request: RequestId,
        identity: SessionRef,
        run_id: RunId,
        expected_revision: u64,
        brief: RunBrief,
        affected: Vec<TaskVersion>,
    ) -> Result<Reply<RunRecord>> {
        self.transact(request,json!({"op":"update_run","caller":identity,"run_id":run_id,"expected_revision":expected_revision,"brief":brief,"affected":affected}),|connection| {
            let mut run = owner(connection,identity,run_id)?;
            if run.plan_revision != expected_revision { return Err(ControlError::new(ErrorCode::RevisionConflict,"the plan revision is outdated")); }
            if brief.title.trim().is_empty() || brief.goal.trim().is_empty() { return Err(invalid("a run needs a title and goal")); }
            super::checks::validate_gate(&brief.quality_gate)?;
            let mut roots = HashSet::new();
            for wanted in affected {
                let task = version(connection,identity,wanted)?;
                if task.run_id != run_id { return Err(ControlError::new(ErrorCode::OwnershipMismatch,"the affected task belongs to another run")); }
                if !roots.insert(task.id) { return Err(invalid("an affected task is listed more than once")); }
            }
            super::dependencies::invalidate_tasks(connection,&roots,true)?;
            run.brief = brief;
            run.plan_revision = run.plan_revision.checked_add(1).ok_or_else(||conflict("the plan revision is exhausted"))?;
            run.state = RunState::Active;
            save_run(connection,&run)?;
            if let Some(candidate) = run.candidate_id { super::checks::refresh(connection,candidate)?; }
            event(connection,Some(run_id),"run_updated",&run)?;
            Ok(run)
        })
    }

    pub fn cancel_run(
        &mut self,
        request: RequestId,
        identity: SessionRef,
        run_id: RunId,
    ) -> Result<Reply<RunRecord>> {
        self.transact(
            request,
            json!({"op":"cancel_run","caller":identity,"run_id":run_id}),
            |connection| {
                let mut run = run_owner(connection, identity, run_id)?;
                match run.state {
                    RunState::Cancelled => return Ok(run),
                    RunState::Succeeded => return Err(conflict("the run already succeeded")),
                    RunState::Cancelling => {
                        settle_cancellation(connection, run_id)?;
                        return require(connection, Table::Runs, run_id);
                    }
                    RunState::Active | RunState::Verifying | RunState::Failed => {}
                }
                for mut task in all::<TaskRecord>(connection, Table::Tasks)?
                    .into_iter()
                    .filter(|task| task.run_id == run_id)
                {
                    if let Some(id) = task.current_attempt {
                        let mut attempt: AttemptRecord = require(connection, Table::Attempts, id)?;
                        if attempt.state.holds_slot() {
                            super::results::request_stop(
                                connection,
                                &task,
                                &mut attempt,
                                "The Leader cancelled this run; stop work and report cancellation",
                            )?;
                            continue;
                        }
                    }
                    if matches!(
                        task.state,
                        TaskState::Planned
                            | TaskState::Queued
                            | TaskState::Blocked
                            | TaskState::Verifying
                    ) {
                        task.state = TaskState::Cancelled;
                        save_task(connection, &task)?;
                        event(connection, Some(run_id), "task_cancelled", &task)?;
                    }
                }
                super::candidates::invalidate_run(connection, run_id)?;
                run.state = RunState::Cancelling;
                save_run(connection, &run)?;
                event(connection, Some(run_id), "run_cancel_requested", &run)?;
                settle_cancellation(connection, run_id)?;
                require(connection, Table::Runs, run_id)
            },
        )
    }
}
