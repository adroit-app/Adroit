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
                        TaskState::Planned | TaskState::Queued | TaskState::Blocked
                    ) {
                        task.state = TaskState::Cancelled;
                        save_task(connection, &task)?;
                        event(connection, Some(run_id), "task_cancelled", &task)?;
                    }
                }
                run.state = RunState::Cancelling;
                save_run(connection, &run)?;
                event(connection, Some(run_id), "run_cancel_requested", &run)?;
                settle_cancellation(connection, run_id)?;
                require(connection, Table::Runs, run_id)
            },
        )
    }
}
