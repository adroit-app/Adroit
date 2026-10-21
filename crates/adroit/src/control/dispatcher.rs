//! Native notifications run outside the database owner and serialize per recipient.
use std::collections::{HashMap, HashSet};

use adroit_model::*;

use super::{actor::Actor, store::ControlError};

type Result<T> = std::result::Result<T, ControlError>;

pub struct Dispatcher {
    actor: Actor,
    active: HashSet<LogicalSessionId>,
    jobs: tokio::task::JoinSet<Result<()>>,
    routes: HashMap<tokio::task::Id, (SessionRef, MessageId)>,
}

impl Dispatcher {
    pub fn new(actor: Actor) -> Self {
        Self {
            actor,
            active: HashSet::new(),
            jobs: tokio::task::JoinSet::new(),
            routes: HashMap::new(),
        }
    }

    pub async fn tick(&mut self) -> Result<usize> {
        while let Some(completed) = self.jobs.try_join_next_with_id() {
            let (task_id, error) = match completed {
                Ok((id, Ok(()))) => (id, None),
                Ok((id, Err(error))) => (id, Some(error.to_string())),
                Err(error) => (
                    error.id(),
                    Some(format!("the native notifier stopped: {error}")),
                ),
            };
            if let Some((identity, message)) = self.routes.remove(&task_id) {
                self.active.remove(&identity.id);
                if let Some(reason) = error {
                    self.actor
                        .call(move |store| {
                            store
                                .delivery_uncertain(RequestId::new(), identity, message, reason)
                                .map(|_| ())
                        })
                        .await?;
                }
            }
        }
        let snapshot = self.actor.call(|store| store.snapshot()).await?;
        let mut count = 0;
        for session in snapshot.sessions {
            if self.active.len() >= 32 {
                break;
            }
            if session.agent != Agent::Codex
                || session.state != ManagedSessionState::Running
                || self.active.contains(&session.binding.logical_id())
            {
                continue;
            }
            let Some(conversation) = session.conversation_id else {
                continue;
            };
            let Some(cwd) = session.cwd else {
                continue;
            };
            let identity = SessionRef {
                id: session.binding.logical_id(),
                generation: session.generation,
            };
            let pending = self
                .actor
                .call(move |store| {
                    let Some(runtime) = store.runtime(identity)? else {
                        return Ok(None);
                    };
                    let message = store.next_pending_message(identity)?;
                    Ok(message.map(|message| (runtime, message)))
                })
                .await?;
            let Some((runtime, message)) = pending else {
                continue;
            };
            let id = message.id;
            let envelope = self
                .actor
                .call(move |store| store.native_envelope(identity, id))
                .await?;
            let claimed = self
                .actor
                .call(move |store| {
                    store
                        .claim_delivery(RequestId::new(), identity, id)
                        .map(|reply| reply.value)
                })
                .await?;
            if claimed.is_none() {
                continue;
            }
            count += 1;
            self.active.insert(identity.id);
            let actor = self.actor.clone();
            let job = self.jobs.spawn(async move {
                let outcome = crate::execution::deliver(
                    Agent::Codex,
                    runtime.settings,
                    cwd,
                    conversation,
                    envelope,
                )
                .await;
                actor
                    .call(move |store| match outcome {
                        Ok(()) => store
                            .accept_delivery(RequestId::new(), id, identity)
                            .map(|_| ()),
                        Err(error) => store
                            .delivery_uncertain(RequestId::new(), identity, id, error.to_string())
                            .map(|_| ()),
                    })
                    .await
            });
            self.routes.insert(job.id(), (identity, id));
        }
        Ok(count)
    }
}
