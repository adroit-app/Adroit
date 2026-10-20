//! Native terminal facts do not substitute for a Worker's successful report.
use super::*;

pub(super) fn notify(
    connection: &Connection,
    session: &ManagedSession,
    kind: MessageKind,
    reason: &str,
) -> Result<()> {
    let Some(run_id) = session.binding.run_id() else {
        return Ok(());
    };
    let work = session.binding.work();
    let revision = work
        .map(|(_, attempt)| require::<AttemptRecord>(connection, Table::Attempts, attempt))
        .transpose()?
        .map(|attempt| attempt.task_revision);
    store_message(
        connection,
        &AgentMessage {
            id: MessageId::new(),
            run_id,
            from: session.binding.logical_id(),
            to: session.binding.leader_id(),
            task_id: work.map(|(task, _)| task),
            attempt_id: work.map(|(_, attempt)| attempt),
            revision,
            kind,
            body: reason.into(),
            reply_to: None,
            blocks_work: kind == MessageKind::Question,
        },
    )
}

pub(super) fn notify_uncertain(
    connection: &Connection,
    session: &ManagedSession,
    reason: &str,
) -> Result<()> {
    let active = match session.binding.work() {
        Some((_, id)) => require::<AttemptRecord>(connection, Table::Attempts, id)?
            .state
            .holds_slot(),
        None => true,
    };
    notify(
        connection,
        session,
        if active {
            MessageKind::Question
        } else {
            MessageKind::Note
        },
        reason,
    )
}

impl Store {
    pub fn require_available_session(&self, identity: SessionRef) -> Result<()> {
        let session = reference(&self.connection, identity)?;
        if matches!(
            session.state,
            ManagedSessionState::NeedsReconciliation
                | ManagedSessionState::Exited
                | ManagedSessionState::Failed
        ) {
            return Err(conflict(
                "the session needs reconciliation before it can change orchestration state",
            ));
        }
        Ok(())
    }

    pub fn observe_terminal_missing(
        &mut self,
        request: RequestId,
        identity: SessionRef,
        terminal: String,
    ) -> Result<Reply<ManagedSession>> {
        self.transact(request,json!({"op":"observe_terminal_missing","caller":identity,"terminal":terminal}),|connection| {
            let mut session=reference(connection,identity)?;
            if session.terminal_id.as_ref()!=Some(&terminal) {return Err(ControlError::new(ErrorCode::OwnershipMismatch,"the observation belongs to another terminal"));}
            if session.state!=ManagedSessionState::Running {return Ok(session);}
            session.state=ManagedSessionState::NeedsReconciliation;
            if let Some((_,id))=session.binding.work() {
                let mut attempt:AttemptRecord=require(connection,Table::Attempts,id)?;
                if attempt.state.holds_slot() && attempt.state!=AttemptState::CancelRequested {
                    attempt.state=AttemptState::Uncertain;
                    save_attempt(connection,&attempt)?;
                }
            }
            save_session(connection,&session)?;
            notify_uncertain(connection,&session,"Adroit cannot find the assigned native terminal. Reconcile its state before continuing; no replacement Worker has been started.")?;
            event(connection,session.binding.run_id(),"session_terminal_missing",&session)?;
            Ok(session)
        })
    }

    pub fn observe_terminal_exit(
        &mut self,
        request: RequestId,
        identity: SessionRef,
        terminal: String,
    ) -> Result<Reply<ManagedSession>> {
        self.transact(request,json!({"op":"observe_terminal_exit","caller":identity,"terminal":terminal}),|connection| {
            let mut session=reference(connection,identity)?;
            if session.terminal_id.as_ref()!=Some(&terminal) {return Err(ControlError::new(ErrorCode::OwnershipMismatch,"the observation belongs to another terminal"));}
            if session.state==ManagedSessionState::Exited {return Ok(session);}
            if !matches!(session.state,ManagedSessionState::Running|ManagedSessionState::NeedsReconciliation) {return Err(conflict("the session has no running terminal to confirm"));}
            if let Some((task_id,id))=session.binding.work() {
                let mut attempt:AttemptRecord=require(connection,Table::Attempts,id)?;
                if attempt.state.holds_slot() {
                    let mut task:TaskRecord=require(connection,Table::Tasks,task_id)?;
                    let run:RunRecord=require(connection,Table::Runs,task.run_id)?;
                    let cancelled=attempt.state==AttemptState::CancelRequested || matches!(run.state,RunState::Cancelling|RunState::Cancelled);
                    attempt.state=if cancelled {AttemptState::Cancelled} else {AttemptState::Failed};
                    save_attempt(connection,&attempt)?;
                    if task.current_attempt==Some(id) {
                        if task.revision==attempt.task_revision || run.state==RunState::Cancelling {
                            task.state=if cancelled {TaskState::Cancelled} else {TaskState::Failed};
                        } else { task.current_attempt=None; }
                        save_task(connection,&task)?;
                    }
                    event(connection,Some(task.run_id),"attempt_terminal_exited",&attempt)?;
                    super::runs::settle_cancellation(connection,task.run_id)?;
                }
            }
            session.state=ManagedSessionState::Exited;
            save_session(connection,&session)?;
            notify(connection,&session,MessageKind::Note,"Adroit confirmed that the assigned native terminal exited. An unfinished attempt is failed or cancelled; terminal exit does not establish successful work.")?;
            event(connection,session.binding.run_id(),"session_terminal_exited",&session)?;
            Ok(session)
        })
    }
}

pub(super) fn fail_start_record(
    connection: &Connection,
    identity: SessionRef,
    reason: &str,
    stopped_terminal: Option<&str>,
) -> Result<ManagedSession> {
    if reason.trim().is_empty() {
        return Err(invalid("a failed startup needs a reason"));
    }
    let mut session = reference(connection, identity)?;
    if let Some(terminal) = stopped_terminal {
        if terminal.len() != 32 || crate::sessions::id_of(terminal).is_none() {
            return Err(invalid("the aborted startup needs its native terminal ID"));
        }
        if session
            .terminal_id
            .as_deref()
            .is_some_and(|held| held != terminal)
        {
            return Err(ControlError::new(
                ErrorCode::OwnershipMismatch,
                "the stopped startup belongs to another terminal",
            ));
        }
    }
    if session.state == ManagedSessionState::Failed
        || (stopped_terminal.is_some() && session.state == ManagedSessionState::Exited)
    {
        return Ok(session);
    }
    if session.state != ManagedSessionState::Preparing
        && !(stopped_terminal.is_some()
            && matches!(
                session.state,
                ManagedSessionState::Running | ManagedSessionState::NeedsReconciliation
            ))
    {
        return Err(conflict("the session is no longer preparing"));
    }
    if let Some(terminal) = stopped_terminal {
        session.terminal_id = Some(terminal.to_owned());
    }
    if let Some((task_id, attempt_id)) = session.binding.work() {
        let mut task: TaskRecord = require(connection, Table::Tasks, task_id)?;
        let mut attempt: AttemptRecord = require(connection, Table::Attempts, attempt_id)?;
        if task.current_attempt != Some(attempt_id)
            || !(matches!(
                attempt.state,
                AttemptState::Preparing | AttemptState::Starting | AttemptState::CancelRequested
            ) || (stopped_terminal.is_some()
                && matches!(
                    attempt.state,
                    AttemptState::Running | AttemptState::Uncertain
                )))
        {
            return Err(conflict("the worker assignment changed"));
        }
        let cancelled = attempt.state == AttemptState::CancelRequested;
        store_message(
            connection,
            &AgentMessage {
                id: MessageId::new(),
                run_id: task.run_id,
                from: identity.id,
                to: session.binding.leader_id(),
                task_id: Some(task.id),
                attempt_id: Some(attempt.id),
                revision: Some(attempt.task_revision),
                kind: MessageKind::Report,
                body: if cancelled {
                    format!("Worker startup cancelled before release: {reason}")
                } else {
                    format!("Worker startup failed: {reason}")
                },
                reply_to: None,
                blocks_work: false,
            },
        )?;
        attempt.state = if cancelled {
            AttemptState::Cancelled
        } else {
            AttemptState::Failed
        };
        let run: RunRecord = require(connection, Table::Runs, task.run_id)?;
        if run.state == RunState::Cancelling {
            task.state = TaskState::Cancelled;
        } else if task.revision == attempt.task_revision {
            task.state = if cancelled {
                TaskState::Cancelled
            } else {
                TaskState::Failed
            };
        } else {
            task.current_attempt = None;
        }
        save_attempt(connection, &attempt)?;
        save_task(connection, &task)?;
        runs::settle_cancellation(connection, task.run_id)?;
        event(
            connection,
            session.binding.run_id(),
            "attempt_start_failed",
            &json!({"attempt":attempt, "reason":reason}),
        )?;
    } else if let Some(run_id) = session.binding.run_id() {
        let mut run: RunRecord = require(connection, Table::Runs, run_id)?;
        if session.generation == 1 && matches!(run.state, RunState::Active | RunState::Verifying) {
            run.state = RunState::Failed;
            save_run(connection, &run)?;
        }
        event(
            connection,
            Some(run_id),
            "run_start_failed",
            &json!({"run":run, "reason":reason}),
        )?;
    }
    session.state = ManagedSessionState::Failed;
    save_session(connection, &session)?;
    event(
        connection,
        session.binding.run_id(),
        "session_start_failed",
        &json!({"session":session, "reason":reason}),
    )?;
    Ok(session)
}

impl Store {
    pub fn restart_leader(
        &mut self,
        request: RequestId,
        identity: SessionRef,
    ) -> Result<Reply<ManagedSession>> {
        self.transact(request, json!({"op":"restart_leader","caller":identity}), |connection| {
            let mut session = reference(connection, identity)?;
            if session.binding.role() != SessionRole::Leader {
                return Err(invalid("only a Leader can restart with its existing request"));
            }
            if !matches!(session.state, ManagedSessionState::Exited | ManagedSessionState::Failed) {
                return Err(conflict("confirm that the assigned native session stopped before restarting the Leader"));
            }
            let runtime: String = connection.query_row(
                "SELECT data FROM session_launches WHERE session_id=?1 AND generation=?2",
                params![identity.id.to_string(), identity.generation.to_string()],
                |row| row.get(0),
            ).optional()?.ok_or_else(|| conflict("the Leader has no saved native routing settings"))?;
            session.generation = session.generation.checked_add(1).ok_or_else(|| conflict("session generation exhausted"))?;
            session.state = ManagedSessionState::Preparing;
            session.terminal_id = None;
            session.conversation_id = None;
            save_session(connection, &session)?;
            connection.execute("DELETE FROM session_credentials WHERE session_id=?1", [identity.id.to_string()])?;
            connection.execute("INSERT INTO session_launches(session_id,generation,data) VALUES (?1,?2,?3)", params![identity.id.to_string(),session.generation.to_string(),runtime])?;
            let dispatching = {
                let mut statement = connection.prepare("SELECT data FROM deliveries WHERE recipient=?1 AND json_extract(data,'$.state')='dispatching'")?;
                statement.query_map([identity.id.to_string()], |row| row.get::<_,String>(0))?.collect::<std::result::Result<Vec<_>,_>>()?
            };
            for data in dispatching {
                let mut record: DeliveryRecord = decode(&data)?;
                if record.state == DeliveryState::Dispatching {
                    record.state = DeliveryState::Uncertain;
                    record.last_error = Some("the recipient generation ended during delivery; inspect its mailbox before retrying".into());
                    save_delivery(connection, &record)?;
                    event(connection, session.binding.run_id(), "delivery_uncertain", &record)?;
                }
            }
            event(connection, session.binding.run_id(), "leader_restart_queued", &session)?;
            Ok(session)
        })
    }

    pub fn interrupted_startups(&self) -> Result<Vec<SessionRef>> {
        let mut statement=self.connection.prepare("SELECT data FROM logical_sessions WHERE json_extract(data,'$.state')='preparing' ORDER BY rowid")?;
        statement
            .query_map([], |row| row.get::<_, String>(0))?
            .map(|row| Ok(decode::<ManagedSession>(&row?)?.reference()))
            .collect()
    }

    pub fn recover_startup(
        &mut self,
        request: RequestId,
        identity: SessionRef,
    ) -> Result<Reply<ManagedSession>> {
        self.transact(request,json!({"op":"recover_startup","caller":identity}),|connection| {
            let mut session=reference(connection,identity)?;
            if session.state!=ManagedSessionState::Preparing {return Ok(session);}
            if let Some((_,id))=session.binding.work() {
                let mut attempt:AttemptRecord=require(connection,Table::Attempts,id)?;
                let start_intent:bool=connection.query_row("SELECT EXISTS(SELECT 1 FROM events WHERE kind='attempt_starting' AND json_extract(data,'$.id')=?1)",[id.to_string()],|row|row.get(0))?;
                if attempt.state==AttemptState::Starting || (attempt.state==AttemptState::CancelRequested && start_intent) {
                    session.state=ManagedSessionState::NeedsReconciliation;
                    if attempt.state!=AttemptState::CancelRequested {attempt.state=AttemptState::Uncertain;}
                    save_attempt(connection,&attempt)?;
                    save_session(connection,&session)?;
                    notify_uncertain(connection,&session,"Startup was interrupted after recording startup intent. Reconcile the native session before continuing; no replacement has been started.")?;
                    event(connection,session.binding.run_id(),"startup_recovery_uncertain",&session)?;
                    return Ok(session);
                }
                if let Some(id)=attempt.work_area_id {
                    let mut area:WorkArea=require(connection,Table::WorkAreas,id)?;
                    if area.state==WorkAreaState::Preparing {
                        area.state=WorkAreaState::NeedsReconciliation;
                        area.error=Some("Working area preparation was interrupted; preserve its files and reconcile its native state".into());
                        connection.execute("UPDATE work_areas SET data=?1 WHERE id=?2",params![encode(&area)?,id.to_string()])?;
                        event(connection,session.binding.run_id(),"work_area_recovery_required",&area)?;
                    }
                }
            }
            fail_start_record(connection,identity,"Startup was interrupted before the native program was released",None)
        })
    }
}

impl Store {
    pub fn abort_prepared_start(
        &mut self,
        request: RequestId,
        identity: SessionRef,
        terminal: String,
        reason: String,
    ) -> Result<Reply<ManagedSession>> {
        self.transact(request,json!({"op":"abort_prepared_start","caller":identity,"terminal":terminal,"reason":reason}),|connection|fail_start_record(connection,identity,&reason,Some(&terminal)))
    }
}
