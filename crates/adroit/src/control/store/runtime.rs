//! Private provider settings and durable transport uncertainty.
use super::*;
use crate::control::runtime::RuntimeLaunch;

impl Store {
    pub fn runtime(&self, identity: SessionRef) -> Result<Option<RuntimeLaunch>> {
        reference(&self.connection, identity)?;
        let data: Option<String> = self
            .connection
            .query_row(
                "SELECT data FROM session_launches WHERE session_id = ?1 AND generation = ?2",
                params![identity.id.to_string(), identity.generation.to_string()],
                |row| row.get(0),
            )
            .optional()?;
        data.as_deref().map(decode).transpose()
    }

    pub fn observe_conversation(
        &mut self,
        request: RequestId,
        identity: SessionRef,
        terminal: String,
        conversation: String,
    ) -> Result<Reply<ManagedSession>> {
        self.transact(request,json!({"op":"observe_conversation","caller":identity,"terminal":terminal,"conversation":conversation}),|connection| {
            let mut session = reference(connection,identity)?;
            if session.terminal_id.as_ref() != Some(&terminal) {
                return Err(ControlError::new(ErrorCode::OwnershipMismatch,"the observation belongs to another terminal"));
            }
            if conversation.parse::<MessageId>().is_err() { return Err(invalid("the native conversation must be a UUID")); }
            if !matches!(session.state,ManagedSessionState::Running | ManagedSessionState::NeedsReconciliation) {
                return Err(conflict("the session has not started or has already exited"));
            }
            match &session.conversation_id {
                Some(held) if *held == conversation => return Ok(session),
                Some(_) if session.state == ManagedSessionState::NeedsReconciliation => return Ok(session),
                Some(_) => {
                    session.state = ManagedSessionState::NeedsReconciliation;
                    if let Some((_,attempt_id)) = session.binding.work() {
                        let mut attempt: AttemptRecord = require(connection,Table::Attempts,attempt_id)?;
                        if attempt.state.holds_slot() && attempt.state != AttemptState::CancelRequested {
                            attempt.state = AttemptState::Uncertain;
                            save_attempt(connection,&attempt)?;
                        }
                    }
                    save_session(connection,&session)?;
                    event(connection,session.binding.run_id(),"session_conversation_changed",&json!({"session":session,"observed_conversation":conversation}))?;
                    super::lifecycle::notify_uncertain(connection,&session,"The native conversation changed. Reconcile the assignment before continuing; Adroit has not transferred task ownership.")?;
                }
                None => {
                    session.conversation_id = Some(conversation);
                    save_session(connection,&session)?;
                    event(connection,session.binding.run_id(),"session_conversation_learned",&session)?;
                }
            }
            Ok(session)
        })
    }

    pub fn next_pending_message(&self, identity: SessionRef) -> Result<Option<AgentMessage>> {
        reference(&self.connection, identity)?;
        let data: Option<String> = self.connection.query_row(
            "SELECT messages.data FROM messages JOIN deliveries ON deliveries.message_id = messages.id WHERE deliveries.recipient = ?1 AND json_extract(deliveries.data, '$.state') = 'stored' ORDER BY messages.rowid LIMIT 1",
            [identity.id.to_string()],|row|row.get(0),
        ).optional()?;
        data.as_deref().map(decode).transpose()
    }

    pub fn remember_launch(
        &mut self,
        request: RequestId,
        identity: SessionRef,
        runtime: RuntimeLaunch,
    ) -> Result<Reply<()>> {
        self.transact(request,json!({"op":"remember_launch","caller":identity,"runtime":runtime}),|connection| {
            let session = reference(connection,identity)?;
            if let Some(target) = &runtime.target {
                if target.agent != session.agent || target.permissions != session.permissions {return Err(ControlError::new(ErrorCode::OwnershipMismatch,"the launch selection does not match the session's agent and permissions"));}
                if let Some((_,id))=session.binding.work() {
                    let attempt:AttemptRecord=require(connection,Table::Attempts,id)?;
                    if attempt.target != *target {return Err(conflict("the launch selection does not match the assigned attempt"));}
                }
            }
            let previous: Option<String> = connection.query_row("SELECT data FROM session_launches WHERE session_id = ?1 AND generation = ?2", params![identity.id.to_string(),identity.generation.to_string()],|row|row.get(0)).optional()?;
            if let Some(previous) = previous {
                if decode::<RuntimeLaunch>(&previous)? != runtime { return Err(conflict("the session's native routing snapshot is immutable")); }
                return Ok(());
            }
            if session.state != ManagedSessionState::Preparing { return Err(conflict("the session has already started")); }
            connection.execute("INSERT INTO session_launches(session_id,generation,data) VALUES (?1,?2,?3)",params![identity.id.to_string(),identity.generation.to_string(),encode(&runtime)?])?;
            event(connection,session.binding.run_id(),"session_routing_recorded",&identity)?;
            Ok(())
        })
    }

    pub fn delivery_uncertain(
        &mut self,
        request: RequestId,
        identity: SessionRef,
        message_id: MessageId,
        reason: String,
    ) -> Result<Reply<DeliveryRecord>> {
        self.transact(request,json!({"op":"delivery_uncertain","caller":identity,"message_id":message_id,"reason":reason}),|connection| {
            reference(connection,identity)?;
            let message: AgentMessage = require(connection,Table::Messages,message_id)?;
            let mut record = delivery(connection,&message,identity.id)?;
            if matches!(record.state,DeliveryState::Acknowledged | DeliveryState::Accepted | DeliveryState::Uncertain) { return Ok(record); }
            if record.state != DeliveryState::Dispatching { return Err(conflict("the message is not being dispatched")); }
            record.state = DeliveryState::Uncertain;
            record.last_error = Some(reason);
            save_delivery(connection,&record)?;
            event(connection,Some(message.run_id),"delivery_uncertain",&record)?;
            Ok(record)
        })
    }
}
