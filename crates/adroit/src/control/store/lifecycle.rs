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
