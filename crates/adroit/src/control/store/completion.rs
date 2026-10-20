//! Durable final checks keep filesystem work outside state transactions.
use super::*;

fn save(connection: &Connection, ticket: &ChangeCompletionRequest) -> Result<()> {
    let state = match ticket.state {
        CompletionRequestState::Pending => "pending",
        CompletionRequestState::Succeeded => "succeeded",
        CompletionRequestState::Rejected => "rejected",
    };
    connection.execute("INSERT INTO completion_requests(id,run_id,state,data) VALUES (?1,?2,?3,?4) ON CONFLICT(id) DO UPDATE SET state=excluded.state,data=excluded.data",params![ticket.id.to_string(),ticket.run_id.to_string(),state,encode(ticket)?])?;
    Ok(())
}

impl Store {
    pub fn request_change_completion(
        &mut self,
        request: RequestId,
        identity: SessionRef,
        run_id: RunId,
        expected_revision: u64,
        candidate_id: CandidateId,
        summary: String,
    ) -> Result<Reply<ChangeCompletionRequest>> {
        self.transact(request,json!({"op":"request_change_completion","caller":identity,"run_id":run_id,"expected_revision":expected_revision,"candidate_id":candidate_id,"summary":summary}),|connection| {
            let mut run=owner(connection,identity,run_id)?;
            if run.plan_revision != expected_revision { return Err(ControlError::new(ErrorCode::RevisionConflict,"the plan revision is outdated")); }
            if summary.trim().is_empty() || summary.len()>64*1024 {return Err(invalid("a completion summary must contain at most 64 KiB of nonempty text"));}
            if run.candidate_id != Some(candidate_id) || run.brief.completion_kind != CompletionKind::Change {return Err(conflict("completion needs the selected change candidate"));}
            super::checks::refresh(connection,candidate_id)?;
            let candidate:CandidateRecord=require(connection,Table::Candidates,candidate_id)?;
            if candidate.state != CandidateState::Verified || candidate.proof.is_none() {return Err(conflict("completion needs current confirmed quality evidence"));}
            let pending:bool=connection.query_row("SELECT EXISTS(SELECT 1 FROM completion_requests WHERE run_id=?1 AND state='pending')",[run_id.to_string()],|row|row.get(0))?;
            if pending {return Err(conflict("the run already has a pending completion request"));}
            let ticket=ChangeCompletionRequest {id:request,leader:identity,run_id,plan_revision:expected_revision,candidate_id,summary,state:CompletionRequestState::Pending,error:None};
            save(connection,&ticket)?;
            run.state=RunState::Verifying;
            save_run(connection,&run)?;
            event(connection,Some(run_id),"change_completion_requested",&ticket)?;
            Ok(ticket)
        })
    }

    pub fn pending_completions(&self, limit: u32) -> Result<Vec<ChangeCompletionRequest>> {
        let mut statement = self.connection.prepare(
            "SELECT data FROM completion_requests WHERE state='pending' ORDER BY rowid LIMIT ?1",
        )?;
        statement
            .query_map([limit.min(64)], |row| row.get::<_, String>(0))?
            .map(|row| decode(&row?))
            .collect()
    }

    pub fn record_change_completion(
        &mut self,
        request: RequestId,
        id: RequestId,
        outcome: std::result::Result<CandidateProof, String>,
    ) -> Result<Reply<ChangeCompletionRequest>> {
        self.transact(request,json!({"op":"record_change_completion","id":id,"outcome":outcome}),|connection| {
            let mut ticket:ChangeCompletionRequest=require(connection,Table::CompletionRequests,id)?;
            if ticket.state != CompletionRequestState::Pending {return Ok(ticket);}
            let mut run:RunRecord=require(connection,Table::Runs,ticket.run_id)?;
            let mut candidate:CandidateRecord=require(connection,Table::Candidates,ticket.candidate_id)?;
            let failure=if run.plan_revision != ticket.plan_revision || run.candidate_id != Some(ticket.candidate_id) || !matches!(run.state,RunState::Active|RunState::Verifying) {
                Some("The plan or selected candidate changed during final confirmation".to_owned())
            } else {
                let observed=outcome.as_ref().ok();
                if observed.is_none() || observed.map(CandidateProof::summary) != candidate.proof.as_ref().map(CandidateProof::summary) || observed.is_some_and(|proof|proof.files.version != 1) {
                    let error=outcome.err().unwrap_or_else(||"Candidate inputs changed before final completion".into());
                    candidate.state=CandidateState::Stale;
                    candidate.error=Some(error.clone());
                    super::candidates::save(connection,&candidate)?;
                    event(connection,Some(run.id),"candidate_inputs_changed",&candidate.summary())?;
                    Some(error)
                } else {
                    super::checks::refresh(connection,ticket.candidate_id)?;
                    candidate=require(connection,Table::Candidates,ticket.candidate_id)?;
                    if candidate.state != CandidateState::Verified || super::candidates::source(connection,&candidate.plan).is_err() {
                        Some("The candidate no longer has current confirmed quality evidence".into())
                    } else {
                        match super::runs::finish(connection,ticket.leader,ticket.run_id,ticket.plan_revision,ticket.summary.clone(),Some(ticket.candidate_id)) {
                            Ok(_) => None,
                            Err(error) if matches!(error.code, ErrorCode::StorageFailure | ErrorCode::CorruptStore) => return Err(error),
                            Err(error) => Some(error.message),
                        }
                    }
                }
            };
            ticket.state=if failure.is_some() {CompletionRequestState::Rejected} else {CompletionRequestState::Succeeded};
            ticket.error=failure;
            if ticket.state == CompletionRequestState::Rejected && run.state == RunState::Verifying {
                run.state=RunState::Active;
                save_run(connection,&run)?;
            }
            save(connection,&ticket)?;
            event(connection,Some(run.id),"change_completion_finished",&ticket)?;
            store_message(connection,&AgentMessage {id:MessageId::new(),run_id:run.id,from:run.leader_id,to:run.leader_id,task_id:None,attempt_id:None,revision:None,kind:MessageKind::Note,body:encode(&json!({"type":"change_completion_finished","actor":"adroit_controller","request_id":ticket.id,"candidate_id":ticket.candidate_id,"state":ticket.state,"error":ticket.error}))?,reply_to:None,blocks_work:false})?;
            Ok(ticket)
        })
    }
}
