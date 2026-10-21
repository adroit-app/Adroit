//! Human-authorized creation of independent managed requests.
use super::*;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct IndependentAuthorization {
    pub id: RequestId,
    pub owner: SessionRef,
    pub workspace: PathBuf,
    pub brief: RunBrief,
    pub agent: Agent,
    pub model: Option<String>,
    pub permissions: AgentPermissions,
    pub created: Option<IndependentRun>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct IndependentRun {
    pub run_id: RunId,
    pub leader_id: LogicalSessionId,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct IndependentAuthorizationPlan {
    pub leader: SessionRef,
    pub workspace: PathBuf,
    pub brief: RunBrief,
    pub agent: Agent,
    pub model: Option<String>,
    pub permissions: AgentPermissions,
}

impl Store {
    pub fn authorize_independent_run(
        &mut self,
        request: RequestId,
        plan: IndependentAuthorizationPlan,
    ) -> Result<Reply<IndependentAuthorization>> {
        let IndependentAuthorizationPlan {
            leader: owner,
            workspace,
            brief,
            agent,
            model,
            permissions,
        } = plan;
        self.transact(request, json!({"op":"authorize_independent_run","owner":owner,"workspace":workspace,"brief":brief,"agent":agent,"model":model,"permissions":permissions}), |connection| {
            let session = reference(connection, owner)?;
            if session.binding.role() != SessionRole::Leader { return Err(ControlError::new(ErrorCode::OwnershipMismatch, "only a managed Leader can receive independent-run authorization")); }
            let run_id = session.binding.run_id().ok_or_else(|| conflict("the Leader has no run"))?;
            super::owner(connection, owner, run_id)?;
            if brief.title.trim().is_empty() || brief.goal.trim().is_empty() { return Err(invalid("an independent run needs a title and goal")); }
            checks::validate_gate(&brief.quality_gate)?;
            if model.as_ref().is_some_and(|model| model.trim().is_empty()) { return Err(invalid("the selected model is empty")); }
            let workspace = std::fs::canonicalize(&workspace).map_err(|error| invalid(format!("cannot resolve the authorized workspace: {error}")))?;
            if !workspace.is_dir() { return Err(invalid("the authorized workspace must be a directory")); }
            let grant = IndependentAuthorization { id: RequestId::new(), owner, workspace, brief, agent, model, permissions, created: None };
            connection.execute("INSERT INTO independent_authorizations(id,leader_id,state,data) VALUES (?1,?2,'authorized',?3)", params![grant.id.to_string(),owner.id.to_string(),encode(&grant)?])?;
            store_message(connection, &AgentMessage { id: MessageId::new(),run_id,from:owner.id,to:owner.id,task_id:None,attempt_id:None,revision:None,kind:MessageKind::Note,body:format!("Human authorized an independent run in {}. Use create_independent_run with authorization_id {} only if you intend to hand off this exact request.",grant.workspace.display(),grant.id),reply_to:None,blocks_work:false })?;
            event(connection, Some(run_id), "independent_run_authorized", &grant)?;
            Ok(grant)
        })
    }

    pub fn create_independent_run(
        &mut self,
        request: RequestId,
        caller: SessionRef,
        authorization_id: RequestId,
    ) -> Result<Reply<IndependentRun>> {
        self.transact(request, json!({"op":"create_independent_run","caller":caller,"authorization_id":authorization_id}), |connection| {
            let session = reference(connection, caller)?;
            if session.binding.role() != SessionRole::Leader { return Err(ControlError::new(ErrorCode::OwnershipMismatch, "only the authorized Leader can create an independent run")); }
            let data: String = connection.query_row("SELECT data FROM independent_authorizations WHERE id=?1",[authorization_id.to_string()],|row|row.get(0)).optional()?.ok_or_else(|| ControlError::new(ErrorCode::NotFound,"no independent-run authorization"))?;
            let mut grant: IndependentAuthorization = decode(&data)?;
            if grant.owner != caller { return Err(ControlError::new(ErrorCode::OwnershipMismatch,"the authorization belongs to another Leader generation")); }
            if grant.created.is_some() { return Err(conflict("the authorization was already used; retry the original request ID")); }
            let origin = session.binding.run_id().ok_or_else(|| conflict("the Leader has no run"))?;
            owner(connection, caller, origin)?;
            let created = IndependentRun { run_id: RunId::new(), leader_id: LogicalSessionId::new() };
            let leader = ManagedSession { binding: SessionBinding::leader(created.leader_id,Some(created.run_id)),agent:grant.agent,generation:1,state:ManagedSessionState::Preparing,permissions:grant.permissions.clone(),cwd:Some(grant.workspace.clone()),terminal_id:None,conversation_id:None };
            let run = RunRecord { id:created.run_id,leader_id:created.leader_id,brief:grant.brief.clone(),plan_revision:1,candidate_id:None,state:RunState::Active };
            save_session(connection,&leader)?;
            save_run(connection,&run)?;
            grant.created = Some(created.clone());
            connection.execute("UPDATE independent_authorizations SET state='created',data=?1 WHERE id=?2",params![encode(&grant)?,authorization_id.to_string()])?;
            event(connection,Some(created.run_id),"independent_run_created",&created)?;
            Ok(created)
        })
    }

    pub fn pending_independent_leaders(
        &self,
    ) -> Result<Vec<(ManagedSession, IndependentAuthorization)>> {
        let mut statement = self.connection.prepare(
            "SELECT data FROM independent_authorizations WHERE state='created' ORDER BY rowid",
        )?;
        let grants = statement
            .query_map([], |row| row.get::<_, String>(0))?
            .collect::<std::result::Result<Vec<_>, _>>()?;
        grants
            .into_iter()
            .map(|data| {
                let grant: IndependentAuthorization = decode(&data)?;
                let created = grant.created.as_ref().ok_or_else(|| {
                    ControlError::new(ErrorCode::CorruptStore, "created authorization has no run")
                })?;
                let session: ManagedSession =
                    require(&self.connection, Table::Sessions, created.leader_id)?;
                Ok((session, grant))
            })
            .collect()
    }
}
