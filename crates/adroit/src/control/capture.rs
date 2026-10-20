//! Bounded, restartable candidate reads outside the orchestration state owner.
use super::{actor::Actor, store::ControlError};
use adroit_model::*;
use std::collections::HashMap;

type Outcome = std::result::Result<CandidateProof, String>;
type Result<T> = std::result::Result<T, ControlError>;
const MAX_JOBS: usize = 4;

pub struct CaptureSupervisor {
    actor: Actor,
    jobs: tokio::task::JoinSet<(CandidateId, Outcome)>,
    active: HashMap<tokio::task::Id, CandidateId>,
}

impl CaptureSupervisor {
    pub fn new(actor: Actor) -> Self {
        Self {
            actor,
            jobs: tokio::task::JoinSet::new(),
            active: HashMap::new(),
        }
    }

    pub async fn tick(&mut self) -> Result<Vec<CandidateId>> {
        while let Some(result) = self.jobs.try_join_next_with_id() {
            let (id, outcome) = match result {
                Ok((job, (id, outcome))) => {
                    self.active.remove(&job);
                    (id, outcome)
                }
                Err(error) => {
                    let Some(id) = self.active.remove(&error.id()) else {
                        continue;
                    };
                    (id, Err(format!("candidate capture stopped: {error}")))
                }
            };
            self.finish(id, outcome).await?;
        }
        let pending = self
            .actor
            .call(|store| store.pending_candidates(64))
            .await?;
        let mut started = Vec::new();
        for candidate in pending {
            if self.active.len() >= MAX_JOBS {
                break;
            }
            let id = candidate.plan.id;
            if self.active.values().any(|active| *active == id) {
                continue;
            }
            let area_id = match &candidate.artifact {
                TaskArtifact::Change { work_area_id, .. } => *work_area_id,
                TaskArtifact::Report { .. } => {
                    self.finish(id, Err("the candidate lost its change artifact".into()))
                        .await?;
                    continue;
                }
            };
            let area = match self.actor.call(move |store| store.work_area(area_id)).await {
                Ok(area) => area,
                Err(error) => {
                    self.finish(id, Err(error.to_string())).await?;
                    continue;
                }
            };
            let job = self.jobs.spawn(async move {
                (
                    id,
                    crate::candidates::capture_candidate(
                        &area,
                        &candidate.artifact,
                        &candidate.plan.inputs,
                    )
                    .await,
                )
            });
            self.active.insert(job.id(), id);
            started.push(id);
        }
        Ok(started)
    }

    async fn finish(&self, id: CandidateId, outcome: Outcome) -> Result<()> {
        self.actor
            .call(move |store| {
                store
                    .record_candidate_capture(RequestId::new(), id, outcome)
                    .map(|_| ())
            })
            .await
    }
}
