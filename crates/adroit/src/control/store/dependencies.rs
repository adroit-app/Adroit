//! Input changes invalidate dependent results without stopping independent work.
use super::*;
use std::collections::HashSet;

pub(super) fn current_inputs(
    connection: &Connection,
    task: &TaskRecord,
    attempt: &AttemptRecord,
) -> Result<bool> {
    if task.dependencies.len() != attempt.input_versions.len() {
        return Ok(false);
    }
    for id in &task.dependencies {
        let Some(wanted) = attempt
            .input_versions
            .iter()
            .find(|input| input.task_id == *id)
        else {
            return Ok(false);
        };
        let dependency: TaskRecord = require(connection, Table::Tasks, id)?;
        if dependency.revision != wanted.revision || dependency.state != TaskState::Succeeded {
            return Ok(false);
        }
    }
    Ok(true)
}

pub(super) fn invalidate_dependents(connection: &Connection, changed: TaskId) -> Result<()> {
    let tasks = all::<TaskRecord>(connection, Table::Tasks)?;
    let mut invalidated = HashSet::from([changed]);
    loop {
        let mut progress = false;
        for original in &tasks {
            if invalidated.contains(&original.id)
                || !original
                    .dependencies
                    .iter()
                    .any(|id| invalidated.contains(id))
            {
                continue;
            }
            let mut task = original.clone();
            if let Some(id) = task.current_attempt {
                let mut attempt: AttemptRecord = require(connection, Table::Attempts, id)?;
                if attempt.state.holds_slot() {
                    super::results::request_stop(
                        connection,
                        &task,
                        &mut attempt,
                        "A task input changed; stop and report cancellation",
                    )?;
                } else {
                    task.current_attempt = None;
                }
            }
            task.revision = task
                .revision
                .checked_add(1)
                .ok_or_else(|| conflict("the task revision is exhausted"))?;
            if task.state != TaskState::Cancelled {
                task.state = TaskState::Planned;
            }
            save_task(connection, &task)?;
            event(
                connection,
                Some(task.run_id),
                "task_inputs_invalidated",
                &task,
            )?;
            invalidated.insert(task.id);
            progress = true;
        }
        if !progress {
            return Ok(());
        }
    }
}
