//! Working-area identity and startup prerequisites; no filesystem exclusion.
use super::*;

fn associated(connection: &Connection, run_id: RunId, area_id: WorkAreaId) -> Result<()> {
    let found: bool = connection.query_row(
        "SELECT EXISTS(SELECT 1 FROM run_work_areas WHERE run_id = ?1 AND work_area_id = ?2)",
        params![run_id.to_string(), area_id.to_string()],
        |row| row.get(0),
    )?;
    if !found {
        return Err(ControlError::new(
            ErrorCode::OwnershipMismatch,
            "the working area is not associated with this run",
        ));
    }
    Ok(())
}

impl Store {
    pub fn register_work_area(
        &mut self,
        request: RequestId,
        identity: SessionRef,
        run_id: RunId,
        area: WorkArea,
        is_default: bool,
    ) -> Result<Reply<WorkArea>> {
        self.transact(request, json!({"op":"register_work_area", "caller":identity, "run_id":run_id, "area":area, "is_default":is_default}), |connection| {
            owner(connection, identity, run_id)?;
            if !area.path.is_absolute() || !area.source.is_absolute() || !area.source_root.is_absolute()
                || !matches!(area.state, WorkAreaState::Preparing | WorkAreaState::Ready) {
                return Err(invalid("a working-area plan needs absolute paths and a preparable state"));
            }
            let kept = all::<WorkArea>(connection, Table::WorkAreas)?.into_iter().find(|kept| kept.path == area.path || kept.id == area.id);
            let kept = match kept {
                Some(kept) => {
                    if kept.path != area.path || kept.backend != area.backend || kept.repository != area.repository
                        || (kept.id != area.id && kept.backend != WorkAreaBackend::Directory && !(is_default && kept.state == WorkAreaState::Ready && area.state == WorkAreaState::Ready)) {
                        return Err(conflict("the working-area identity is already registered with different metadata"));
                    }
                    kept
                }
                None => {
                    connection.execute("INSERT INTO work_areas(id, path, data) VALUES (?1, ?2, ?3)", params![area.id.to_string(), area.path.to_string_lossy(), encode(&area)?])?;
                    area
                }
            };
            if is_default {
                let other: bool = connection.query_row("SELECT EXISTS(SELECT 1 FROM run_work_areas WHERE run_id = ?1 AND is_default = 1 AND work_area_id != ?2)", params![run_id.to_string(), kept.id.to_string()], |row| row.get(0))?;
                if other { return Err(conflict("the run already has a different default working area")); }
            }
            let association = RunWorkArea { run_id, work_area_id: kept.id, is_default };
            let existing: Option<String> = connection.query_row("SELECT data FROM run_work_areas WHERE run_id = ?1 AND work_area_id = ?2", params![run_id.to_string(), kept.id.to_string()], |row| row.get(0)).optional()?;
            if let Some(existing) = existing {
                let existing: RunWorkArea = decode(&existing)?;
                if is_default && !existing.is_default { return Err(conflict("the working-area association has a different default selection")); }
            } else {
                connection.execute("INSERT INTO run_work_areas(run_id, work_area_id, is_default, data) VALUES (?1, ?2, ?3, ?4)", params![run_id.to_string(), kept.id.to_string(), is_default, encode(&association)?])?;
                event(connection, Some(run_id), "work_area_registered", &json!({"area":kept, "association":association}))?;
                super::coordination::subscribe(connection, &association, &kept)?;
            }
            Ok(kept)
        })
    }

    pub fn finish_work_area(
        &mut self,
        request: RequestId,
        area_id: WorkAreaId,
        outcome: std::result::Result<(), String>,
    ) -> Result<Reply<WorkArea>> {
        self.transact(
            request,
            json!({"op":"finish_work_area", "area_id":area_id, "outcome":outcome}),
            |connection| {
                let mut area: WorkArea = require(connection, Table::WorkAreas, area_id)?;
                let (state, error) = match outcome {
                    Ok(()) => (WorkAreaState::Ready, None),
                    Err(error) => (WorkAreaState::Failed, Some(error)),
                };
                if area.state == state && area.error == error {
                    return Ok(area);
                }
                if area.state != WorkAreaState::Preparing {
                    return Err(conflict("the working area is no longer preparing"));
                }
                area.state = state;
                area.error = error;
                connection.execute(
                    "UPDATE work_areas SET data = ?1 WHERE id = ?2",
                    params![encode(&area)?, area.id.to_string()],
                )?;
                for association in all::<RunWorkArea>(connection, Table::RunWorkAreas)?
                    .into_iter()
                    .filter(|association| association.work_area_id == area_id)
                {
                    event(
                        connection,
                        Some(association.run_id),
                        "work_area_prepared",
                        &area,
                    )?;
                }
                Ok(area)
            },
        )
    }

    pub fn bind_work_area(
        &mut self,
        request: RequestId,
        identity: SessionRef,
        wanted: TaskVersion,
        attempt_id: AttemptId,
        area_id: WorkAreaId,
    ) -> Result<Reply<AttemptRecord>> {
        self.transact(request, json!({"op":"bind_work_area", "caller":identity, "task":wanted, "attempt_id":attempt_id, "area_id":area_id}), |connection| {
            let task = version(connection, identity, wanted)?;
            let mut attempt: AttemptRecord = require(connection, Table::Attempts, attempt_id)?;
            if task.current_attempt != Some(attempt_id) || attempt.task_id != task.id || attempt.task_revision != task.revision || attempt.state != AttemptState::Preparing {
                return Err(conflict("the attempt no longer matches the task preparation"));
            }
            associated(connection, task.run_id, area_id)?;
            if let Some(candidate) = super::candidates::task_candidate(connection,task.run_id,&task.brief,&task.dependencies,true)? {
                let proof = candidate.proof.as_ref().expect("prepared candidate");
                if proof.work_area_id != area_id { return Err(invalid("a candidate check cannot bind a different working area")); }
            }
            let area: WorkArea = require(connection, Table::WorkAreas, area_id)?;
            if !matches!(area.state, WorkAreaState::Preparing | WorkAreaState::Ready) { return Err(conflict("the working area is unavailable")); }
            if let Some(existing) = attempt.work_area_id {
                if existing != area_id { return Err(conflict("the attempt already has a different working area")); }
                return Ok(attempt);
            }
            attempt.work_area_id = Some(area_id);
            attempt.base_revision = area.base_revision;
            save_attempt(connection, &attempt)?;
            event(connection, Some(task.run_id), "attempt_work_area_bound", &attempt)?;
            Ok(attempt)
        })
    }

    pub fn begin_start(
        &mut self,
        request: RequestId,
        identity: SessionRef,
        wanted: TaskVersion,
        attempt_id: AttemptId,
    ) -> Result<Reply<AttemptRecord>> {
        self.transact(
            request,
            json!({"op":"begin_start", "caller":identity, "task":wanted, "attempt_id":attempt_id}),
            |connection| {
                let task = version(connection, identity, wanted)?;
                let mut attempt: AttemptRecord = require(connection, Table::Attempts, attempt_id)?;
                if task.current_attempt != Some(attempt_id)
                    || attempt.task_id != task.id
                    || attempt.task_revision != task.revision
                    || !matches!(
                        attempt.state,
                        AttemptState::Preparing | AttemptState::Starting
                    )
                {
                    return Err(conflict("the attempt no longer matches the task startup"));
                }
                if let Some(candidate) = super::candidates::task_candidate(
                    connection,
                    task.run_id,
                    &task.brief,
                    &task.dependencies,
                    true,
                )? && attempt.candidate_input
                    != candidate.proof.as_ref().map(CandidateProof::summary)
                {
                    return Err(conflict(
                        "the check needs a confirmed candidate input before startup",
                    ));
                }
                let area_id = attempt
                    .work_area_id
                    .ok_or_else(|| conflict("the attempt has no working area"))?;
                associated(connection, task.run_id, area_id)?;
                let area: WorkArea = require(connection, Table::WorkAreas, area_id)?;
                if area.state != WorkAreaState::Ready {
                    return Err(conflict("the working area is not ready"));
                }
                if attempt.state == AttemptState::Preparing {
                    attempt.state = AttemptState::Starting;
                    save_attempt(connection, &attempt)?;
                    event(connection, Some(task.run_id), "attempt_starting", &attempt)?;
                }
                Ok(attempt)
            },
        )
    }
}
