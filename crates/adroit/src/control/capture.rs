//! Bounded, restartable candidate reads outside the orchestration state owner.
use super::{actor::Actor, store::ControlError};
use adroit_model::*;
use std::collections::HashMap;

type Outcome = std::result::Result<CandidateProof, String>;
type Result<T> = std::result::Result<T, ControlError>;
const MAX_JOBS: usize = 4;

#[derive(Clone, Copy, PartialEq, Eq)]
enum Capture {
    Candidate(CandidateId),
    Check(AttemptId),
    Completion(RequestId),
}

pub struct CaptureSupervisor {
    actor: Actor,
    jobs: tokio::task::JoinSet<(Capture, Outcome)>,
    active: HashMap<tokio::task::Id, Capture>,
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
            let capture = Capture::Candidate(id);
            if self.active.values().any(|active| *active == capture) {
                continue;
            }
            let area_id = match &candidate.artifact {
                TaskArtifact::Change { work_area_id, .. } => *work_area_id,
                TaskArtifact::Report { .. } | TaskArtifact::Review { .. } => {
                    self.finish(
                        capture,
                        Err("the candidate lost its change artifact".into()),
                    )
                    .await?;
                    continue;
                }
            };
            let area = match self.actor.call(move |store| store.work_area(area_id)).await {
                Ok(area) => area,
                Err(error) => {
                    self.finish(capture, Err(error.to_string())).await?;
                    continue;
                }
            };
            let job = self.jobs.spawn(async move {
                (
                    capture,
                    crate::candidates::capture_candidate(
                        &area,
                        &candidate.artifact,
                        &candidate.plan.inputs,
                    )
                    .await,
                )
            });
            self.active.insert(job.id(), capture);
            started.push(id);
        }
        let pending = self
            .actor
            .call(|store| store.pending_candidate_checks(64))
            .await?;
        for check in pending {
            if self.active.len() >= MAX_JOBS {
                break;
            }
            let capture = Capture::Check(check.attempt_id);
            if self.active.values().any(|active| *active == capture) {
                continue;
            }
            let id = check.candidate_id;
            let source = self
                .actor
                .call(move |store| {
                    let candidate = store.candidate(id)?;
                    let area_id = match candidate.artifact {
                        TaskArtifact::Change { work_area_id, .. } => work_area_id,
                        _ => {
                            return Err(ControlError::new(
                                super::store::ErrorCode::InvalidRequest,
                                "the candidate lost its change artifact",
                            ));
                        }
                    };
                    Ok((store.work_area(area_id)?, candidate))
                })
                .await;
            let (area, candidate) = match source {
                Ok(source) => source,
                Err(error) => {
                    self.finish(capture, Err(error.to_string())).await?;
                    continue;
                }
            };
            let job = self.jobs.spawn(async move {
                (
                    capture,
                    crate::candidates::capture_candidate(
                        &area,
                        &candidate.artifact,
                        &candidate.plan.inputs,
                    )
                    .await,
                )
            });
            self.active.insert(job.id(), capture);
        }
        let pending = self
            .actor
            .call(|store| store.pending_completions(64))
            .await?;
        for ticket in pending {
            if self.active.len() >= MAX_JOBS {
                break;
            }
            let capture = Capture::Completion(ticket.id);
            if self.active.values().any(|active| *active == capture) {
                continue;
            }
            let id = ticket.candidate_id;
            let source = self
                .actor
                .call(move |store| {
                    let candidate = store.candidate(id)?;
                    let area_id = match candidate.artifact {
                        TaskArtifact::Change { work_area_id, .. } => work_area_id,
                        _ => {
                            return Err(ControlError::new(
                                super::store::ErrorCode::InvalidRequest,
                                "the candidate lost its change artifact",
                            ));
                        }
                    };
                    Ok((store.work_area(area_id)?, candidate))
                })
                .await;
            let (area, candidate) = match source {
                Ok(source) => source,
                Err(error) => {
                    self.finish(capture, Err(error.to_string())).await?;
                    continue;
                }
            };
            let job = self.jobs.spawn(async move {
                (
                    capture,
                    crate::candidates::capture_candidate(
                        &area,
                        &candidate.artifact,
                        &candidate.plan.inputs,
                    )
                    .await,
                )
            });
            self.active.insert(job.id(), capture);
        }
        Ok(started)
    }

    async fn finish(&self, capture: Capture, outcome: Outcome) -> Result<()> {
        self.actor
            .call(move |store| match capture {
                Capture::Candidate(id) => store
                    .record_candidate_capture(RequestId::new(), id, outcome)
                    .map(|_| ()),
                Capture::Check(id) => store
                    .record_candidate_check(RequestId::new(), id, outcome)
                    .map(|_| ()),
                Capture::Completion(id) => store
                    .record_change_completion(RequestId::new(), id, outcome)
                    .map(|_| ()),
            })
            .await
    }
}
