//! A bounded assignment read independent of the run's message history.
use super::*;

impl Store {
    pub fn context(&mut self, identity: SessionRef) -> Result<AgentContext> {
        let transaction = self.connection.transaction()?;
        current(&transaction)?;
        let session = reference(&transaction, identity)?;
        let run = session
            .binding
            .run_id()
            .map(|id| require::<RunRecord>(&transaction, Table::Runs, id))
            .transpose()?;
        let (task, attempt) = match session.binding.work() {
            Some((task, attempt)) => {
                let task: TaskRecord = require(&transaction, Table::Tasks, task)?;
                let attempt: AttemptRecord = require(&transaction, Table::Attempts, attempt)?;
                if Some(task.run_id) != session.binding.run_id()
                    || attempt.task_id != task.id
                    || attempt.session_id != identity.id
                {
                    return Err(ControlError::new(
                        ErrorCode::CorruptStore,
                        "the stored assignment context is inconsistent",
                    ));
                }
                (Some(task), Some(attempt))
            }
            None => (None, None),
        };
        let area = if let Some(attempt) = &attempt {
            attempt.work_area_id
        } else if let Some(run) = &run {
            all::<RunWorkArea>(&transaction, Table::RunWorkAreas)?
                .into_iter()
                .find(|association| association.run_id == run.id && association.is_default)
                .map(|association| association.work_area_id)
        } else {
            None
        };
        let work_area = area
            .map(|id| require(&transaction, Table::WorkAreas, id))
            .transpose()?;
        transaction.commit()?;
        Ok(AgentContext {
            session,
            run,
            task,
            attempt,
            work_area,
        })
    }
}
