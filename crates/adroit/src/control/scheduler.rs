//! Background preparation and startup. Slots count attempts, never directories.
use std::path::PathBuf;

use adroit_model::*;
use adroit_term::Daemon;

use super::{
    actor::Actor,
    store::{ControlError, ErrorCode, UnreadableRun},
};
use crate::{execution, settings::Settings, workareas};

type Result<T> = std::result::Result<T, ControlError>;

pub struct Scheduler {
    actor: Actor,
    daemon: Daemon,
    root: PathBuf,
    program: PathBuf,
    jobs: tokio::task::JoinSet<()>,
    active: std::collections::HashMap<tokio::task::Id, SessionRef>,
    last_run: Option<RunId>,
    unreadable: Vec<UnreadableRun>,
}

impl Scheduler {
    pub fn new(actor: Actor, daemon: Daemon, root: PathBuf, program: PathBuf) -> Self {
        Self {
            actor,
            daemon,
            root,
            program,
            jobs: tokio::task::JoinSet::new(),
            active: std::collections::HashMap::new(),
            last_run: None,
            unreadable: Vec::new(),
        }
    }

    /// The runs the last tick left alone, as a record of theirs cannot be read.
    pub fn unreadable(&self) -> &[UnreadableRun] {
        &self.unreadable
    }

    pub async fn tick(&mut self, settings: Settings, max_workers: usize) -> Result<Vec<AttemptId>> {
        while let Some(result) = self.jobs.try_join_next_with_id() {
            match result {
                Ok((job, ())) => {
                    self.active.remove(&job);
                }
                Err(error) => {
                    if let Some(identity) = self.active.remove(&error.id()) {
                        self.actor
                            .call(move |store| {
                                store
                                    .recover_startup(RequestId::new(), identity)
                                    .map(|_| ())
                            })
                            .await?;
                    }
                }
            }
        }
        let readable = self.actor.call(|store| store.readable_snapshot()).await?;
        self.unreadable = readable.unreadable;
        let held_slots = readable.held_slots;
        let snapshot = readable.snapshot;
        let independent = self
            .actor
            .call(|store| store.pending_independent_leaders())
            .await?;
        for (session, grant) in independent
            .into_iter()
            .filter(|(session, _)| {
                session.state == ManagedSessionState::Preparing
                    && !self
                        .active
                        .values()
                        .any(|identity| *identity == session.reference())
            })
            .take(4)
            .collect::<Vec<_>>()
        {
            let identity = session.reference();
            let actor = self.actor.clone();
            let daemon = self.daemon.clone();
            let root = self.root.clone();
            let program = self.program.clone();
            let settings = settings.clone();
            let job = self.jobs.spawn(async move {
                let result = async {
                    grant.validate_workspace()?;
                    let target = execution::select_target(&settings, grant.agent, grant.model.clone(), grant.permissions.clone()).map_err(failed)?;
                    let mut runtime = super::runtime::RuntimeLaunch::new(settings.clone()).with_target(target);
                    runtime.developer_instructions = crate::agents::adapter(grant.agent)
                        .developer_instructions(runtime.settings.clone(), grant.workspace.clone()).await.map_err(failed)?;
                    let captured = runtime.clone();
                    actor.call(move |store| store.remember_launch(RequestId::new(), identity, captured).map(|_| ())).await?;
                    let area = workareas::existing(WorkAreaId::new(), grant.workspace.clone(), "Independent Leader workspace".into()).await.map_err(failed)?;
                    let run_id = grant.created.as_ref().expect("created authorization").run_id;
                    actor.call(move |store| store.register_work_area(RequestId::new(), identity, run_id, area, true).map(|_| ())).await?;
                    let prompt = format!("This is a new independent request explicitly authorized by the human. Read Adroit context and snapshot first. Work only on this run and its workspace. Request: {}\n\n{}", grant.brief.title, grant.brief.goal);
                    grant.validate_workspace()?;
                    super::restart::start(&actor, &daemon, &root, &program, session, Some(prompt)).await
                }.await;
                if let Err(error) = result {
                    let _ = actor.call(move |store| store.fail_start(RequestId::new(), identity, error.to_string()).map(|_| ())).await;
                }
            });
            self.active.insert(job.id(), identity);
        }
        let restarting = snapshot
            .sessions
            .iter()
            .filter(|session| {
                session.binding.role() == SessionRole::Leader
                    && self
                        .active
                        .values()
                        .any(|identity| *identity == session.reference())
            })
            .count();
        for session in snapshot
            .sessions
            .iter()
            .filter(|session| {
                session.binding.role() == SessionRole::Leader
                    && session.generation > 1
                    && session.state == ManagedSessionState::Preparing
                    && !self
                        .active
                        .values()
                        .any(|identity| *identity == session.reference())
            })
            .take(4usize.saturating_sub(restarting))
            .cloned()
            .collect::<Vec<_>>()
        {
            let identity = session.reference();
            let actor = self.actor.clone();
            let daemon = self.daemon.clone();
            let root = self.root.clone();
            let program = self.program.clone();
            let job = self.jobs.spawn(async move {
                if let Err(error) =
                    super::restart::start(&actor, &daemon, &root, &program, session, None).await
                {
                    let _ = actor
                        .call(move |store| {
                            store
                                .fail_start(RequestId::new(), identity, error.to_string())
                                .map(|_| ())
                        })
                        .await;
                }
            });
            self.active.insert(job.id(), identity);
        }
        let occupied = snapshot
            .attempts
            .iter()
            .filter(|attempt| attempt.state.holds_slot())
            .count()
            + held_slots;
        let mut remaining = max_workers.saturating_sub(occupied);
        let per_run = settings.orchestration.max_workers_per_run.min(64);
        let mut run_occupied = std::collections::HashMap::<RunId, usize>::new();
        for attempt in snapshot
            .attempts
            .iter()
            .filter(|attempt| attempt.state.holds_slot())
        {
            if let Some(task) = snapshot
                .tasks
                .iter()
                .find(|task| task.id == attempt.task_id)
            {
                *run_occupied.entry(task.run_id).or_default() += 1;
            }
        }
        let mut runs: Vec<_> = snapshot
            .runs
            .iter()
            .filter(|run| matches!(run.state, RunState::Active | RunState::Verifying))
            .collect();
        if let Some(index) = runs.iter().position(|run| Some(run.id) == self.last_run) {
            let length = runs.len();
            runs.rotate_left((index + 1) % length);
        }
        let mut ready: Vec<_> = runs
            .iter()
            .map(|run| {
                snapshot
                    .tasks
                    .iter()
                    .filter(|task| {
                        task.run_id == run.id
                            && task.state == TaskState::Queued
                            && task.current_attempt.is_none()
                            && task.brief.candidate_id.is_none_or(|id| {
                                snapshot.candidates.iter().any(|candidate| {
                                    candidate.plan.id == id
                                        && candidate.plan.run_id == run.id
                                        && run.candidate_id == Some(id)
                                        && matches!(
                                            candidate.state,
                                            CandidateState::Prepared | CandidateState::Verified
                                        )
                                })
                            })
                            && task.dependencies.iter().all(|id| {
                                snapshot.tasks.iter().any(|dependency| {
                                    dependency.id == *id && dependency.state == TaskState::Succeeded
                                })
                            })
                    })
                    .collect::<std::collections::VecDeque<_>>()
            })
            .collect();
        let mut launched = Vec::new();
        while remaining > 0 {
            let mut found = false;
            for tasks in &mut ready {
                if tasks.front().is_some_and(|task| {
                    run_occupied.get(&task.run_id).copied().unwrap_or_default() >= per_run
                }) {
                    continue;
                }
                let Some(task) = tasks.pop_front() else {
                    continue;
                };
                found = true;
                let run = snapshot
                    .runs
                    .iter()
                    .find(|run| run.id == task.run_id)
                    .expect("selected active run");
                let leader = snapshot
                    .sessions
                    .iter()
                    .find(|session| session.binding.logical_id() == run.leader_id)
                    .ok_or_else(|| failed("the task's Leader is missing"))?;
                let agent = task.brief.target.agent.unwrap_or(settings.agent);
                let selected = execution::select_target(
                    &settings,
                    agent,
                    task.brief.target.model.clone(),
                    task.brief.target.permissions.clone(),
                );
                let selection_error = selected.as_ref().err().cloned();
                let target = selected.unwrap_or_else(|_| TargetSelection {
                    agent,
                    requested_model: task.brief.target.model.clone(),
                    permissions: task.brief.target.permissions.clone(),
                    ..Default::default()
                });
                let wanted = TaskVersion {
                    task_id: task.id,
                    revision: task.revision,
                };
                let owner = leader.reference();
                let attempt = self
                    .actor
                    .call(move |store| {
                        store
                            .reserve_attempt(RequestId::new(), owner, wanted, target)
                            .map(|reply| reply.value)
                    })
                    .await?;
                self.last_run = Some(run.id);
                launched.push(attempt.id);
                if let Some(reason) = selection_error {
                    let identity = SessionRef {
                        id: attempt.session_id,
                        generation: 1,
                    };
                    self.actor
                        .call(move |store| {
                            store
                                .fail_start(RequestId::new(), identity, reason)
                                .map(|_| ())
                        })
                        .await?;
                    continue;
                }
                remaining -= 1;
                *run_occupied.entry(run.id).or_default() += 1;
                let context = Startup {
                    actor: self.actor.clone(),
                    daemon: self.daemon.clone(),
                    root: self.root.clone(),
                    program: self.program.clone(),
                    settings: settings.clone(),
                    owner,
                    task: task.clone(),
                    attempt,
                    source: leader.cwd.clone(),
                };
                let identity = SessionRef {
                    id: context.attempt.session_id,
                    generation: 1,
                };
                let job = self.jobs.spawn(async move {
                    if let Err(error) = context.start().await {
                        let identity = SessionRef {
                            id: context.attempt.session_id,
                            generation: 1,
                        };
                        let reason = error.to_string();
                        let _ = context
                            .actor
                            .call(move |store| {
                                store
                                    .fail_start(RequestId::new(), identity, reason)
                                    .map(|_| ())
                            })
                            .await;
                    }
                });
                self.active.insert(job.id(), identity);
                if remaining == 0 {
                    break;
                }
            }
            if !found {
                break;
            }
        }
        Ok(launched)
    }
}

struct Startup {
    actor: Actor,
    daemon: Daemon,
    root: PathBuf,
    program: PathBuf,
    settings: Settings,
    owner: SessionRef,
    task: TaskRecord,
    attempt: AttemptRecord,
    source: Option<PathBuf>,
}

impl Startup {
    async fn start(&self) -> Result<()> {
        let source = self
            .task
            .brief
            .source_dir
            .clone()
            .or_else(|| self.source.clone())
            .ok_or_else(|| failed("the task has no source directory"))?;
        let wanted = TaskVersion {
            task_id: self.task.id,
            revision: self.task.revision,
        };
        let owner = self.owner;
        let run_id = self.task.run_id;
        let attempt_id = self.attempt.id;
        let area = match self.task.brief.workdir {
            WorkdirPolicy::Auto
                if self.attempt.target.permissions == AgentPermissions::ReadOnly
                    && self.task.brief.candidate_id.is_none()
                    && self.task.brief.base_revision.is_none() =>
            {
                let identified =
                    workareas::identify(WorkAreaId::new(), source, "Read-only task source".into())
                        .await
                        .map_err(failed)?;
                self.actor
                    .call(move |store| {
                        store
                            .register_source_work_area(RequestId::new(), owner, run_id, identified)
                            .map(|reply| reply.value)
                    })
                    .await?
            }
            WorkdirPolicy::Auto => {
                let planned = workareas::plan(workareas::WorkAreaRequest {
                    id: WorkAreaId::new(),
                    source,
                    destination: None,
                    revision: self.task.brief.base_revision.clone(),
                    description: format!(
                        "adroit: {}",
                        self.task
                            .brief
                            .instructions
                            .lines()
                            .next()
                            .unwrap_or("worker task")
                    ),
                })
                .await
                .map_err(failed)?;
                let area = self
                    .actor
                    .call(move |store| {
                        store
                            .register_work_area(RequestId::new(), owner, run_id, planned, false)
                            .map(|reply| reply.value)
                    })
                    .await?;
                self.bind(wanted, &area).await?;
                if area.state == WorkAreaState::Preparing {
                    let outcome = workareas::prepare(&area).await.map(|_| ());
                    let id = area.id;
                    self.actor
                        .call(move |store| {
                            store
                                .finish_work_area(RequestId::new(), id, outcome)
                                .map(|reply| reply.value)
                        })
                        .await?
                } else {
                    area
                }
            }
            WorkdirPolicy::Existing(id) => {
                let snapshot = self
                    .actor
                    .call(|store| store.readable_snapshot())
                    .await?
                    .snapshot;
                let area = snapshot
                    .work_areas
                    .into_iter()
                    .find(|area| area.id == id)
                    .ok_or_else(|| failed("the selected working area is missing"))?;
                if snapshot.run_work_areas.iter().any(|association| {
                    association.run_id == run_id && association.work_area_id == id
                }) {
                    area
                } else {
                    self.actor
                        .call(move |store| {
                            store
                                .register_work_area(RequestId::new(), owner, run_id, area, false)
                                .map(|reply| reply.value)
                        })
                        .await?
                }
            }
        };
        if area.state != WorkAreaState::Ready {
            return Err(failed(
                area.error
                    .unwrap_or_else(|| "the working area is not ready".into()),
            ));
        }
        self.bind(wanted, &area).await?;
        let identity = SessionRef {
            id: self.attempt.session_id,
            generation: 1,
        };
        let binding =
            SessionBinding::worker(identity.id, owner.id, run_id, self.task.id, attempt_id)
                .map_err(failed)?;
        if let Some(id) = self.task.brief.candidate_id {
            let candidate = self.actor.call(move |store| store.candidate(id)).await?;
            let outcome = crate::candidates::capture_candidate(
                &area,
                &candidate.artifact,
                &candidate.plan.inputs,
            )
            .await;
            let recorded = self
                .actor
                .call(move |store| {
                    store
                        .record_candidate_input(
                            RequestId::new(),
                            owner,
                            wanted,
                            attempt_id,
                            outcome,
                        )
                        .map(|reply| reply.value)
                })
                .await?;
            if recorded.is_none() {
                return Err(failed("the candidate inputs changed before Worker startup"));
            }
        }
        let credential = self.actor.call(move |store| store.grant(identity)).await?;
        let mut runtime = super::runtime::RuntimeLaunch::new(self.settings.clone())
            .with_target(self.attempt.target.clone());
        runtime.developer_instructions = crate::agents::adapter(self.attempt.target.agent)
            .developer_instructions(runtime.settings.clone(), area.path.clone())
            .await
            .map_err(failed)?;
        let captured = runtime.clone();
        self.actor
            .call(move |store| {
                store
                    .remember_launch(RequestId::new(), identity, captured)
                    .map(|_| ())
            })
            .await?;
        let resolved = execution::resolve(
            &runtime.settings,
            execution::StartRequest {
                agent: self.attempt.target.agent,
                developer_instructions: runtime.developer_instructions.clone(),
                model: self.attempt.target.requested_model.clone(),
                name: format!("worker-{}", attempt_id),
                cwd: area.path.clone(),
                prompt: format!(
                    "{}\n\nTask revision: {}\nScope: {}\nInputs: {}",
                    self.task.brief.instructions,
                    self.task.revision,
                    self.task.brief.scope.join("\n"),
                    self.task.brief.inputs.join("\n")
                ),
                permissions: self.attempt.target.permissions.clone(),
                binding: Some(binding),
                control: Some(execution::ControlContext::new(
                    identity,
                    self.root.clone(),
                    credential,
                )),
                env: crate::daemon::terminal_env(),
                ..Default::default()
            },
            &self.program,
        )
        .map_err(failed)?;
        let palette = crate::themes::palette(&self.settings.theme, &Settings::themes_dir())
            .map_err(failed)?;
        let conversation = resolved.conversation;
        self.actor
            .call(move |store| {
                store
                    .begin_start(RequestId::new(), owner, wanted, attempt_id)
                    .map(|_| ())
            })
            .await?;
        let actor = self.actor.clone();
        let cwd = area.path;
        let attached = execution::spawn_headless_prepared(
            &self.daemon,
            resolved.spawn,
            &palette,
            move |terminal| async move {
                actor
                    .call(move |store| {
                        store
                            .attach_session(
                                RequestId::new(),
                                identity,
                                cwd,
                                format!("{terminal:032x}"),
                                conversation,
                            )
                            .map(|_| ())
                    })
                    .await
                    .map_err(|error| adroit_term::Error::Protocol(error.to_string()))
            },
        )
        .await;
        if let Err(adroit_term::Error::NotFound(program)) = &attached {
            return Err(failed(format!(
                "the Worker program was not found: {program}"
            )));
        }
        if let Err(adroit_term::Error::PreparationAborted { session, reason }) = &attached {
            let terminal = format!("{session:032x}");
            let reason = reason.clone();
            self.actor
                .call(move |store| {
                    store
                        .abort_prepared_start(RequestId::new(), identity, terminal, reason)
                        .map(|_| ())
                })
                .await?;
            return Ok(());
        }
        if let Err(error) = attached {
            self.actor
                .call(move |store| {
                    store
                        .start_uncertain(RequestId::new(), identity, error.to_string())
                        .map(|_| ())
                })
                .await?;
        }
        Ok(())
    }

    async fn bind(&self, wanted: TaskVersion, area: &WorkArea) -> Result<()> {
        let (owner, attempt, area) = (self.owner, self.attempt.id, area.id);
        self.actor
            .call(move |store| {
                store
                    .bind_work_area(RequestId::new(), owner, wanted, attempt, area)
                    .map(|_| ())
            })
            .await
    }
}

fn failed(error: impl std::fmt::Display) -> ControlError {
    ControlError::new(ErrorCode::StorageFailure, error.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn a_panicking_startup_job_recovers_its_assignment_without_starting_another_program() {
        let dir = tempfile::tempdir().unwrap();
        let actor = Actor::open(dir.path().join("control/store.sqlite3"))
            .await
            .unwrap();
        let cwd = dir.path().to_path_buf();
        let identity = actor
            .call(move |store| {
                store
                    .register_leader(RequestId::new(), LogicalSessionId::new(), Agent::Codex, cwd)
                    .map(|reply| reply.value.reference())
            })
            .await
            .unwrap();
        let mut scheduler = Scheduler::new(
            actor.clone(),
            Daemon::new(dir.path().join("absent.sock")),
            dir.path().into(),
            "/absent-adroit".into(),
        );
        let job = scheduler
            .jobs
            .spawn(async { panic!("interrupted startup fixture") });
        scheduler.active.insert(job.id(), identity);
        while !job.is_finished() {
            tokio::task::yield_now().await;
        }
        scheduler.tick(Settings::default(), 0).await.unwrap();
        let snapshot = actor.call(|store| store.snapshot()).await.unwrap();
        assert_eq!(snapshot.sessions.len(), 1);
        assert_eq!(snapshot.sessions[0].state, ManagedSessionState::Failed);
        assert!(snapshot.attempts.is_empty());
        assert!(scheduler.active.is_empty());
    }
}
