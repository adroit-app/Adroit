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
    invalidate_tasks(connection, &HashSet::from([changed]), false)
}

pub(super) fn invalidate_tasks(
    connection: &Connection,
    roots: &HashSet<TaskId>,
    include_roots: bool,
) -> Result<()> {
    let tasks = all::<TaskRecord>(connection, Table::Tasks)?;
    let mut affected = roots.clone();
    loop {
        let previous = affected.len();
        for task in &tasks {
            if task.dependencies.iter().any(|id| affected.contains(id)) {
                affected.insert(task.id);
            }
        }
        if affected.len() == previous {
            break;
        }
    }
    super::candidates::invalidate_sources(connection, &affected)?;
    for mut task in tasks
        .into_iter()
        .filter(|task| affected.contains(&task.id) && (include_roots || !roots.contains(&task.id)))
    {
        if let Some(id) = task.current_attempt {
            let mut attempt: AttemptRecord = require(connection, Table::Attempts, id)?;
            if attempt.state.holds_slot() {
                super::results::request_stop(
                    connection,
                    &task,
                    &mut attempt,
                    "The request or a task input changed; stop and report cancellation",
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
    }
    Ok(())
}
