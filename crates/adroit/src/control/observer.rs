//! Learn conversations from explicitly tagged terminals, never from names or paths.
use adroit_model::*;
use adroit_term::{Daemon, tags};

use super::{
    actor::Actor,
    store::{ControlError, ErrorCode},
};

type Result<T> = std::result::Result<T, ControlError>;

#[derive(Default)]
pub struct Observer {
    lookups: tokio::task::JoinSet<Option<String>>,
    active: std::collections::HashMap<tokio::task::Id, SessionRef>,
    next: usize,
}

impl Observer {
    pub async fn tick(&mut self, actor: &Actor, daemon: &Daemon) -> Result<()> {
        let mut completed = Vec::new();
        while let Some(result) = self.lookups.try_join_next_with_id() {
            match result {
                Ok((job, conversation)) => {
                    if let Some(identity) = self.active.remove(&job) {
                        completed.push((identity, conversation));
                    }
                }
                Err(error) => {
                    self.active.remove(&error.id());
                }
            }
        }
        let native = tokio::time::timeout(std::time::Duration::from_secs(3), daemon.sessions())
            .await
            .map_err(|_| failed("the terminal daemon did not answer"))?
            .map_err(failed)?;
        let mut snapshot = actor.call(|store| store.snapshot()).await?;
        let count = snapshot.sessions.len();
        if count > 0 {
            snapshot.sessions.rotate_left(self.next % count);
            self.next = (self.next + 1) % count;
        }
        for session in snapshot.sessions.iter().cloned() {
            if !matches!(
                session.state,
                ManagedSessionState::Running | ManagedSessionState::NeedsReconciliation
            ) {
                continue;
            }
            let Some(terminal) = session
                .terminal_id
                .as_deref()
                .and_then(crate::sessions::id_of)
            else {
                continue;
            };
            let Some(native) = native.iter().find(|native| native.id == terminal) else {
                let identity = session.reference();
                let terminal_id = crate::sessions::hex(terminal);
                actor
                    .call(move |store| {
                        store
                            .observe_terminal_missing(RequestId::new(), identity, terminal_id)
                            .map(|_| ())
                    })
                    .await?;
                continue;
            };
            if native.exited {
                let identity = session.reference();
                let terminal_id = crate::sessions::hex(terminal);
                actor
                    .call(move |store| {
                        store
                            .observe_terminal_exit(RequestId::new(), identity, terminal_id)
                            .map(|_| ())
                    })
                    .await?;
                continue;
            }
            let binding = tags::value(&native.tags, "adroit.binding")
                .and_then(|data| serde_json::from_str::<SessionBinding>(&data).ok());
            let generation = tags::value(&native.tags, "adroit.generation")
                .and_then(|value| value.parse::<u64>().ok());
            let Some(binding) = binding else {
                continue;
            };
            if generation != Some(session.generation)
                || binding.logical_id() != session.binding.logical_id()
                || binding.role() != session.binding.role()
                || binding.work() != session.binding.work()
            {
                continue;
            }
            if let Some((_, attempt_id)) = session.binding.work()
                && let Some(attempt) = snapshot
                    .attempts
                    .iter()
                    .find(|attempt| attempt.id == attempt_id)
                && attempt.state == AttemptState::Running
            {
                let screen = daemon.visible(terminal).await.map_err(failed)?;
                let observed = crate::agents::adapter(session.agent)
                    .startup_readiness(&screen)
                    .or_else(|| {
                        let now = std::time::SystemTime::now()
                            .duration_since(std::time::UNIX_EPOCH)
                            .unwrap_or_default()
                            .as_millis() as u64;
                        startup_stalled(attempt.readiness, attempt.spawned_at_ms, now)
                            .then_some(WorkerReadiness::Stalled)
                    });
                if let Some(readiness) =
                    observed.filter(|readiness| *readiness != attempt.readiness)
                {
                    let identity = session.reference();
                    let terminal_id = crate::sessions::hex(terminal);
                    actor
                        .call(move |store| {
                            store
                                .observe_worker_readiness(
                                    RequestId::new(),
                                    identity,
                                    terminal_id,
                                    readiness,
                                )
                                .map(|_| ())
                        })
                        .await?;
                }
            }
            if let Some((_, attempt_id)) = session.binding.work() {
                let finished = snapshot.attempts.iter().any(|attempt| {
                    attempt.id == attempt_id
                        && attempt.session_id == session.binding.logical_id()
                        && !attempt.state.holds_slot()
                });
                let closed_leader = snapshot.sessions.iter().any(|leader| {
                    leader.binding.role() == SessionRole::Leader
                        && leader.binding.logical_id() == session.binding.leader_id()
                        && leader.binding.run_id() == session.binding.run_id()
                        && leader.state == ManagedSessionState::Closed
                });
                if (finished || closed_leader) && binding == session.binding {
                    daemon.end(terminal).await.map_err(failed)?;
                    let identity = session.reference();
                    let terminal_id = crate::sessions::hex(terminal);
                    actor
                        .call(move |store| {
                            store
                                .observe_terminal_exit(RequestId::new(), identity, terminal_id)
                                .map(|_| ())
                        })
                        .await?;
                    continue;
                }
            }
            if session.state != ManagedSessionState::Running {
                continue;
            }
            let adapter = crate::agents::adapter(session.agent);
            let title = native.title.as_deref();
            let conversation = title
                .and_then(|title| adapter.conversation_title(title))
                .or_else(|| {
                    session.conversation_id.clone().filter(|id| {
                        title.is_some_and(|title| adapter.conversation_matches_title(title, id))
                    })
                })
                .or_else(|| {
                    completed
                        .iter()
                        .find(|(identity, _)| *identity == session.reference())
                        .and_then(|(_, id)| id.clone())
                        .filter(|id| {
                            title.is_some_and(|title| adapter.conversation_matches_title(title, id))
                        })
                })
                .or_else(|| {
                    title
                        .is_none_or(|title| !adapter.conversation_title_needs_lookup(title))
                        .then(|| {
                            tags::value(&native.tags, "adroit.claude")
                                .filter(|value| !value.is_empty())
                        })
                        .flatten()
                });
            let Some(conversation) = conversation else {
                if self.active.len() < 4
                    && !self
                        .active
                        .values()
                        .any(|identity| *identity == session.reference())
                    && let Some(title) =
                        title.filter(|title| adapter.conversation_title_needs_lookup(title))
                    && let Some(cwd) = session.cwd.clone()
                {
                    let identity = session.reference();
                    if let Some(runtime) = actor.call(move |store| store.runtime(identity)).await? {
                        let title = title.to_owned();
                        let job = self.lookups.spawn(adapter.resolve_conversation(
                            runtime.settings,
                            cwd,
                            title,
                        ));
                        self.active.insert(job.id(), identity);
                    }
                }
                continue;
            };
            let identity = session.reference();
            let observed = conversation.clone();
            let terminal_id = crate::sessions::hex(terminal);
            let learned = if session.conversation_id.as_ref() == Some(&conversation) {
                session
            } else {
                actor
                    .call(move |store| {
                        store
                            .observe_conversation(RequestId::new(), identity, terminal_id, observed)
                            .map(|reply| reply.value)
                    })
                    .await?
            };
            if learned.state != ManagedSessionState::Running {
                continue;
            }
            if tags::value(&native.tags, "adroit.claude").as_ref() != Some(&conversation)
                || tags::value(&native.tags, "adroit.binding-conversation").as_ref()
                    != Some(&conversation)
                || binding != learned.binding
            {
                let mut add = tags::tagged("adroit.claude", &conversation);
                add.extend(tags::tagged("adroit.binding-conversation", &conversation));
                add.extend(tags::tagged(
                    "adroit.binding",
                    &serde_json::to_string(&learned.binding).map_err(failed)?,
                ));
                let remove = replaced_tags(&native.tags, &add);
                daemon.tag(terminal, add, remove).await.map_err(failed)?;
            }
        }
        Ok(())
    }
}

fn replaced_tags(previous: &[String], added: &[String]) -> Vec<String> {
    [
        "adroit.claude",
        "adroit.binding-conversation",
        "adroit.binding",
    ]
    .into_iter()
    .flat_map(|key| tags::holding(previous, key))
    // Felis applies removals last, so unchanged ownership tags must stay out.
    .filter(|tag| !added.contains(tag))
    .collect()
}

fn startup_stalled(readiness: WorkerReadiness, spawned_at_ms: Option<u64>, now_ms: u64) -> bool {
    readiness == WorkerReadiness::Spawned
        && spawned_at_ms.is_some_and(|start| now_ms.saturating_sub(start) >= 60_000)
}

fn failed(error: impl std::fmt::Display) -> ControlError {
    ControlError::new(ErrorCode::StorageFailure, error.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn startup_stall_requires_an_unready_spawned_worker_and_elapsed_time() {
        assert!(!startup_stalled(
            WorkerReadiness::Spawned,
            Some(100),
            60_099
        ));
        assert!(startup_stalled(WorkerReadiness::Spawned, Some(100), 60_100));
        assert!(!startup_stalled(WorkerReadiness::Ready, Some(100), 80_000));
        assert!(!startup_stalled(
            WorkerReadiness::AwaitingTrust,
            Some(100),
            80_000
        ));
        assert!(!startup_stalled(WorkerReadiness::Spawned, None, 80_000));
    }

    #[test]
    fn discovery_keeps_a_fragmented_worker_binding_in_native_session_projection() {
        let binding = SessionBinding::worker(
            LogicalSessionId::new(),
            LogicalSessionId::new(),
            RunId::new(),
            TaskId::new(),
            AttemptId::new(),
        )
        .unwrap();
        let mut previous = crate::sessions::agent_tags(
            Agent::Codex,
            "worker",
            std::path::Path::new("/repo"),
            "",
            &AgentPermissions::Default,
            1,
            None,
        );
        let binding_tags =
            tags::tagged("adroit.binding", &serde_json::to_string(&binding).unwrap());
        assert!(binding_tags.len() > 1);
        previous.extend(binding_tags.clone());
        previous.extend(tags::tagged("adroit.binding-conversation", ""));
        previous.extend(tags::tagged("adroit.generation", "1"));
        let conversation = MessageId::new().to_string();
        let mut added = tags::tagged("adroit.claude", &conversation);
        added.extend(tags::tagged("adroit.binding-conversation", &conversation));
        added.extend(binding_tags.clone());
        let removed = replaced_tags(&previous, &added);
        let mut updated: std::collections::BTreeSet<_> = previous.into_iter().collect();
        updated.extend(added);
        updated.retain(|tag| !removed.contains(tag));
        let native = adroit_term::Session {
            id: 42,
            tags: updated.into_iter().collect(),
            title: Some(format!("Codex · {conversation}")),
            exited: false,
            status: None,
            sequence: 1,
        };
        assert_eq!(tags::holding(&native.tags, "adroit.binding"), binding_tags);
        assert_eq!(
            tags::value(&native.tags, "adroit.generation").as_deref(),
            Some("1")
        );
        let Some(crate::sessions::Kept::Agent(projected)) = crate::sessions::read(&native) else {
            panic!("discovery must preserve the native Agent projection");
        };
        assert_eq!(projected.binding, Some(binding));
        assert_eq!(projected.session_id, conversation);
    }
}
