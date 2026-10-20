//! Advisory coordination records; no operation grants or holds an execution lease.
use super::*;

fn bounded(value: &impl Serialize) -> Result<()> {
    if encode(value)?.len() > 32 * 1024 {
        return Err(invalid("a coordination record exceeds 32 KiB"));
    }
    Ok(())
}

pub(super) fn subscribe(
    connection: &Connection,
    association: &RunWorkArea,
    area: &WorkArea,
) -> Result<()> {
    let run: RunRecord = require(connection, Table::Runs, association.run_id)?;
    let leader: ManagedSession = require(connection, Table::Sessions, run.leader_id)?;
    if leader.binding.run_id() != Some(run.id) {
        return Ok(());
    }
    for mut notice in all::<CoordinationNotice>(connection, Table::Coordination)? {
        if notice
            .participants
            .iter()
            .any(|participant| participant.run_id == run.id)
        {
            continue;
        }
        let target: WorkArea = require(connection, Table::WorkAreas, notice.intent.work_area_id)?;
        if target.path != area.path
            && !(area.repository.is_some() && area.repository == target.repository)
        {
            continue;
        }
        notice.participants.push(CoordinationParticipant {
            leader: leader.reference(),
            run_id: run.id,
        });
        connection.execute(
            "UPDATE coordination_notices SET data = ?1 WHERE id = ?2",
            params![encode(&notice)?, notice.intent.id.to_string()],
        )?;
        store_message(
            connection,
            &AgentMessage {
                id: MessageId::new(),
                run_id: run.id,
                from: notice.leader.id,
                to: run.leader_id,
                task_id: None,
                attempt_id: None,
                revision: None,
                kind: MessageKind::Note,
                body: encode(&json!({"type":"default_change_context","notice":notice}))?,
                reply_to: None,
                blocks_work: false,
            },
        )?;
        event(connection, Some(run.id), "default_change_context", &notice)?;
    }
    Ok(())
}

fn notify(
    connection: &Connection,
    notice: &CoordinationNotice,
    sender: SessionRef,
    kind: &str,
    data: &impl Serialize,
) -> Result<()> {
    let body =
        encode(&json!({"type":kind,"notice_id":notice.intent.id,"from":sender,"data":data}))?;
    for participant in &notice.participants {
        if participant.leader.id == sender.id {
            continue;
        }
        let session: ManagedSession = require(connection, Table::Sessions, participant.leader.id)?;
        if session.binding.run_id() != Some(participant.run_id) {
            continue;
        }
        // The envelope belongs to the recipient's run; the record preserves the source run.
        store_message(
            connection,
            &AgentMessage {
                id: MessageId::new(),
                run_id: participant.run_id,
                from: sender.id,
                to: participant.leader.id,
                task_id: None,
                attempt_id: None,
                revision: None,
                kind: MessageKind::Note,
                body: body.clone(),
                reply_to: None,
                blocks_work: false,
            },
        )?;
    }
    Ok(())
}

impl Store {
    pub fn announce_change(
        &mut self,
        request: RequestId,
        identity: SessionRef,
        intent: CoordinationIntent,
    ) -> Result<Reply<CoordinationNotice>> {
        self.transact(
            request,
            json!({"op":"announce_change","caller":identity,"intent":intent}),
            |connection| {
                owner(connection, identity, intent.run_id)?;
                bounded(&intent)?;
                if intent.purpose.trim().is_empty()
                    || intent.scope.is_empty()
                    || intent.scope.iter().any(|scope| scope.trim().is_empty())
                {
                    return Err(invalid(
                        "a change intent needs its purpose and concrete scope",
                    ));
                }
                let associations = all::<RunWorkArea>(connection, Table::RunWorkAreas)?;
                if !associations.iter().any(|association| {
                    association.run_id == intent.run_id
                        && association.work_area_id == intent.work_area_id
                        && association.is_default
                }) {
                    return Err(ControlError::new(
                        ErrorCode::OwnershipMismatch,
                        "the notice must identify this run's default working area",
                    ));
                }
                let area: WorkArea = require(connection, Table::WorkAreas, intent.work_area_id)?;
                if area.state != WorkAreaState::Ready {
                    return Err(conflict("the default working area is not ready"));
                }
                if let Some(existing) =
                    load::<CoordinationNotice>(connection, Table::Coordination, intent.id)?
                {
                    if existing.intent != intent || existing.leader != identity {
                        return Err(conflict(
                            "the coordination ID was reused with different content",
                        ));
                    }
                    return Ok(existing);
                }
                let runs = all::<RunRecord>(connection, Table::Runs)?;
                let areas = all::<WorkArea>(connection, Table::WorkAreas)?;
                let mut participants = Vec::new();
                for session in all::<ManagedSession>(connection, Table::Sessions)?
                    .into_iter()
                    .filter(|session| session.binding.role() == SessionRole::Leader)
                {
                    let Some(run_id) = session.binding.run_id() else {
                        continue;
                    };
                    if !runs.iter().any(|run| {
                        run.id == run_id
                            && matches!(
                                run.state,
                                RunState::Active | RunState::Verifying | RunState::Cancelling
                            )
                    }) {
                        continue;
                    }
                    let related = run_id == intent.run_id
                        || session.cwd.as_ref().is_some_and(|cwd| cwd == &area.path)
                        || associations
                            .iter()
                            .filter(|association| association.run_id == run_id)
                            .any(|association| {
                                areas.iter().any(|other| {
                                    other.id == association.work_area_id
                                        && (other.path == area.path
                                            || (area.repository.is_some()
                                                && other.repository == area.repository))
                                })
                            });
                    if related {
                        participants.push(CoordinationParticipant {
                            leader: session.reference(),
                            run_id,
                        });
                    }
                }
                let notice = CoordinationNotice {
                    intent,
                    leader: identity,
                    participants,
                    outcome: None,
                };
                connection.execute(
                    "INSERT INTO coordination_notices(id,run_id,data) VALUES (?1,?2,?3)",
                    params![
                        notice.intent.id.to_string(),
                        notice.intent.run_id.to_string(),
                        encode(&notice)?
                    ],
                )?;
                notify(
                    connection,
                    &notice,
                    identity,
                    "default_change_announced",
                    &notice,
                )?;
                event(
                    connection,
                    Some(notice.intent.run_id),
                    "default_change_announced",
                    &notice,
                )?;
                Ok(notice)
            },
        )
    }

    pub fn discuss_change(
        &mut self,
        request: RequestId,
        identity: SessionRef,
        reply: CoordinationReply,
    ) -> Result<Reply<CoordinationReply>> {
        self.transact(
            request,
            json!({"op":"discuss_change","caller":identity,"reply":reply}),
            |connection| {
                run_owner(connection, identity, reply.run_id)?;
                bounded(&reply)?;
                let notice: CoordinationNotice =
                    require(connection, Table::Coordination, reply.notice_id)?;
                if reply.from != identity
                    || !notice.participants.iter().any(|participant| {
                        participant.leader.id == identity.id && participant.run_id == reply.run_id
                    })
                {
                    return Err(ControlError::new(
                        ErrorCode::OwnershipMismatch,
                        "only a related Leader may discuss this notice",
                    ));
                }
                if reply.body.trim().is_empty() {
                    return Err(invalid("a coordination reply needs an explanation"));
                }
                if let Some(task) = reply.task_id {
                    let task: TaskRecord = require(connection, Table::Tasks, task)?;
                    if task.run_id != reply.run_id {
                        return Err(invalid("the related task belongs to another run"));
                    }
                }
                if let Some(existing) =
                    load::<CoordinationReply>(connection, Table::CoordinationReplies, reply.id)?
                {
                    if existing != reply {
                        return Err(conflict("the reply ID was reused with different content"));
                    }
                    return Ok(existing);
                }
                connection.execute(
                    "INSERT INTO coordination_replies(id,notice_id,data) VALUES (?1,?2,?3)",
                    params![
                        reply.id.to_string(),
                        reply.notice_id.to_string(),
                        encode(&reply)?
                    ],
                )?;
                notify(
                    connection,
                    &notice,
                    identity,
                    "default_change_discussion",
                    &reply,
                )?;
                event(
                    connection,
                    Some(reply.run_id),
                    "default_change_discussion",
                    &reply,
                )?;
                Ok(reply)
            },
        )
    }

    pub fn finish_change(
        &mut self,
        request: RequestId,
        identity: SessionRef,
        notice_id: CoordinationId,
        outcome: CoordinationOutcome,
    ) -> Result<Reply<CoordinationNotice>> {
        self.transact(
            request,
            json!({"op":"finish_change","caller":identity,"notice_id":notice_id,"outcome":outcome}),
            |connection| {
                let mut notice: CoordinationNotice =
                    require(connection, Table::Coordination, notice_id)?;
                run_owner(connection, identity, notice.intent.run_id)?;
                if notice.leader.id != identity.id {
                    return Err(ControlError::new(
                        ErrorCode::OwnershipMismatch,
                        "only the announcing Leader may finish this notice",
                    ));
                }
                bounded(&outcome)?;
                if outcome.summary.trim().is_empty() {
                    return Err(invalid("a change outcome needs a summary"));
                }
                if let Some(existing) = &notice.outcome {
                    if existing != &outcome {
                        return Err(conflict("this notice already has a different outcome"));
                    }
                    return Ok(notice);
                }
                notice.outcome = Some(outcome);
                connection.execute(
                    "UPDATE coordination_notices SET data = ?1 WHERE id = ?2",
                    params![encode(&notice)?, notice_id.to_string()],
                )?;
                notify(
                    connection,
                    &notice,
                    identity,
                    "default_change_finished",
                    &notice,
                )?;
                event(
                    connection,
                    Some(notice.intent.run_id),
                    "default_change_finished",
                    &notice,
                )?;
                Ok(notice)
            },
        )
    }
}
