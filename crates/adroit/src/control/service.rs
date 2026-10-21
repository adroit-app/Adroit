//! Daemon-owned control endpoint and authenticated dispatch.
use std::{
    fs::{DirBuilder, File, OpenOptions},
    os::unix::fs::{DirBuilderExt, MetadataExt, OpenOptionsExt, PermissionsExt},
    path::{Path, PathBuf},
};

use adroit_model::*;
use serde::Serialize;
use serde_json::Value;
use tokio::io::{AsyncBufRead, AsyncBufReadExt, AsyncWriteExt, BufReader};

use super::{
    actor::Actor,
    protocol::*,
    store::{ControlError, ErrorCode, Store},
};

type Result<T> = std::result::Result<T, ControlError>;

pub struct Service {
    task: tokio::task::JoinHandle<()>,
    socket: PathBuf,
    _claim: File,
    actor: Actor,
    supervisor: Option<tokio::task::JoinHandle<()>>,
    startup: Vec<SessionRef>,
}

impl Drop for Service {
    fn drop(&mut self) {
        self.task.abort();
        if let Some(task) = &self.supervisor {
            task.abort();
        }
        let _ = std::fs::remove_file(&self.socket);
    }
}

impl Service {
    pub fn actor(&self) -> Actor {
        self.actor.clone()
    }

    pub fn supervise(
        &mut self,
        daemon: adroit_term::Daemon,
        program: PathBuf,
        settings: impl Fn() -> crate::settings::Settings + Send + Sync + 'static,
    ) -> Result<()> {
        self.supervise_checked(daemon, program, move || Ok(settings()))
    }

    pub fn supervise_checked(
        &mut self,
        daemon: adroit_term::Daemon,
        program: PathBuf,
        settings: impl Fn() -> std::result::Result<crate::settings::Settings, String>
        + Send
        + Sync
        + 'static,
    ) -> Result<()> {
        if self.supervisor.is_some() {
            return Err(ControlError::new(
                ErrorCode::Conflict,
                "the control supervisor is already running",
            ));
        }
        let actor = self.actor.clone();
        let startup = std::mem::take(&mut self.startup);
        let root = self
            .socket
            .parent()
            .expect("control endpoint directory")
            .to_path_buf();
        self.supervisor = Some(tokio::spawn(async move {
            for identity in startup {
                if let Err(error) = actor
                    .call(move |store| {
                        store
                            .recover_startup(RequestId::new(), identity)
                            .map(|_| ())
                    })
                    .await
                {
                    eprintln!("adroit: startup recovery: {error}");
                }
            }
            let mut scheduler =
                super::scheduler::Scheduler::new(actor.clone(), daemon.clone(), root, program);
            let mut observer = super::observer::Observer::default();
            let mut dispatcher = super::dispatcher::Dispatcher::new(actor.clone());
            let mut captures = super::capture::CaptureSupervisor::new(actor.clone());
            let mut interval = tokio::time::interval(std::time::Duration::from_millis(250));
            interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
            let mut last_error = None;
            loop {
                interval.tick().await;
                let result = async {
                    observer.tick(&actor, &daemon).await?;
                    captures.tick().await?;
                    dispatcher.tick().await?;
                    let settings = settings()
                        .map_err(|error| ControlError::new(ErrorCode::InvalidRequest, error))?
                        .sane();
                    let capacity = settings.orchestration.max_workers;
                    scheduler.tick(settings, capacity).await?;
                    Ok::<_, ControlError>(())
                }
                .await;
                match result {
                    Ok(()) => last_error = None,
                    Err(error) => {
                        let message = error.to_string();
                        if last_error.as_ref() != Some(&message) {
                            eprintln!("adroit: orchestration supervision: {message}");
                            last_error = Some(message);
                        }
                    }
                }
            }
        }));
        Ok(())
    }

    pub async fn bind(root: &Path) -> Result<Self> {
        DirBuilder::new()
            .recursive(true)
            .mode(0o700)
            .create(root)
            .map_err(storage)?;
        let metadata = std::fs::symlink_metadata(root).map_err(storage)?;
        // SAFETY: geteuid only reads the current process identity.
        if !metadata.is_dir()
            || metadata.mode() & 0o077 != 0
            || metadata.uid() != unsafe { libc::geteuid() }
        {
            return Err(storage(
                "the control directory must be private and owned by the current user",
            ));
        }
        let claim = File::open(root).map_err(storage)?;
        claim.try_lock().map_err(storage)?;
        let human = human_token(root)?;
        let actor = Actor::open(root.join("state.sqlite3")).await?;
        let startup = actor.call(|store| store.interrupted_startups()).await?;
        let socket = root.join("control.sock");
        match std::fs::remove_file(&socket) {
            Ok(()) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(storage(error)),
        }
        let listener = tokio::net::UnixListener::bind(&socket).map_err(storage)?;
        std::fs::set_permissions(&socket, std::fs::Permissions::from_mode(0o600))
            .map_err(storage)?;
        let serving_actor = actor.clone();
        let task = tokio::spawn(async move {
            let actor = serving_actor;
            let mut clients = tokio::task::JoinSet::new();
            let permits = std::sync::Arc::new(tokio::sync::Semaphore::new(128));
            loop {
                tokio::select! {
                    accepted = listener.accept() => match accepted {
                        Ok((stream, _)) => {
                            let Ok(permit) = permits.clone().try_acquire_owned() else { continue; };
                            let (actor, human) = (actor.clone(), human.clone());
                            clients.spawn(async move {
                                let _permit = permit;
                                let _ = tokio::time::timeout(TIMEOUT, reply(stream, actor, human)).await;
                            });
                        }
                        Err(_) => break,
                    },
                    _ = clients.join_next(), if !clients.is_empty() => {},
                }
            }
        });
        Ok(Self {
            task,
            socket,
            _claim: claim,
            actor,
            supervisor: None,
            startup,
        })
    }
}

fn human_token(root: &Path) -> Result<String> {
    use std::io::Write;
    let path = root.join("human.token");
    match OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(&path)
    {
        Ok(mut file) => {
            let token = format!("{}{}", RequestId::new(), RequestId::new());
            file.write_all(token.as_bytes()).map_err(storage)?;
            file.sync_all().map_err(storage)?;
            Ok(token)
        }
        Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
            let metadata = std::fs::symlink_metadata(&path).map_err(storage)?;
            // SAFETY: geteuid only reads the current process identity.
            if !metadata.is_file()
                || metadata.mode() & 0o077 != 0
                || metadata.uid() != unsafe { libc::geteuid() }
            {
                return Err(storage(
                    "the human credential must be a private regular file",
                ));
            }
            let token = std::fs::read_to_string(path).map_err(storage)?;
            if token.len() != 72 {
                return Err(storage("the human credential is corrupt"));
            }
            Ok(token)
        }
        Err(error) => Err(storage(error)),
    }
}

pub(crate) async fn read_frame(reader: &mut (impl AsyncBufRead + Unpin)) -> Result<Vec<u8>> {
    let mut frame = Vec::new();
    loop {
        let available = reader.fill_buf().await.map_err(storage)?;
        if available.is_empty() {
            return Err(invalid("the control frame is incomplete"));
        }
        let end = available.iter().position(|byte| *byte == b'\n');
        let count = end.map_or(available.len(), |index| index + 1);
        if frame.len() + count > MAX_FRAME {
            return Err(invalid("the control frame is too large"));
        }
        frame.extend_from_slice(&available[..count]);
        reader.consume(count);
        if end.is_some() {
            return Ok(frame);
        }
    }
}

async fn reply(stream: tokio::net::UnixStream, actor: Actor, human: String) -> Result<()> {
    let (read, mut write) = stream.into_split();
    let request = match read_frame(&mut BufReader::new(read)).await {
        Ok(frame) => serde_json::from_slice::<Request>(&frame)
            .map_err(|_| invalid("invalid control request")),
        Err(error) => Err(error),
    };
    let id = request.as_ref().ok().map(|r| r.id);
    let result = match request {
        Ok(request) if request.version == VERSION => {
            actor
                .call(move |store| dispatch(store, &human, request))
                .await
        }
        Ok(_) => Err(invalid("unsupported control protocol version")),
        Err(error) => Err(error),
    };
    let response = Response {
        version: VERSION,
        id,
        result,
    };
    let mut bytes = serde_json::to_vec(&response).map_err(storage)?;
    if bytes.len() >= MAX_FRAME {
        bytes = serde_json::to_vec(&Response {
            version: VERSION,
            id,
            result: Err(invalid("the response exceeds the control frame limit")),
        })
        .map_err(storage)?;
    }
    bytes.push(b'\n');
    write.write_all(&bytes).await.map_err(storage)
}

fn dispatch(store: &mut Store, human: &str, request: Request) -> Result<Value> {
    let is_human = request.credential == human;
    let actor = if is_human {
        request.actor
    } else {
        if request.actor.is_some() {
            return Err(denied("agent credentials cannot select another actor"));
        }
        Some(store.authenticate(&request.credential)?)
    };
    let identity = || actor.ok_or_else(|| denied("select a session for this operation"));
    if !is_human
        && !matches!(
            &request.command,
            Command::Context
                | Command::Snapshot
                | Command::Mailbox { .. }
                | Command::ReadMessage { .. }
                | Command::Events { .. }
                | Command::Candidate { .. }
        )
    {
        store.require_available_session(identity()?)?;
    }
    match request.command {
        Command::RegisterLeader {
            id,
            agent,
            cwd,
            permissions,
        } if is_human => {
            value(store.register_leader_launch(request.id, id, agent, cwd, permissions)?)
        }
        Command::RememberLaunch { identity, runtime } if is_human => {
            value(store.remember_launch(request.id, identity, *runtime)?)
        }
        Command::RegisterDefaultArea {
            identity,
            run_id,
            area,
        } if is_human => value(store.register_work_area(request.id, identity, run_id, area, true)?),
        Command::AttachSession {
            identity,
            cwd,
            terminal_id,
            conversation_id,
        } if is_human => {
            value(store.attach_session(request.id, identity, cwd, terminal_id, conversation_id)?)
        }
        Command::ConfirmTerminalExit {
            identity,
            terminal_id,
        } if is_human => value(store.observe_terminal_exit(request.id, identity, terminal_id)?),
        Command::RestartLeader { identity } if is_human => {
            value(store.restart_leader(request.id, identity)?)
        }
        Command::AbortPreparedStart {
            identity,
            terminal_id,
            reason,
        } if is_human => {
            value(store.abort_prepared_start(request.id, identity, terminal_id, reason)?)
        }
        Command::StartUncertain { identity, reason } if is_human => {
            value(store.start_uncertain(request.id, identity, reason)?)
        }
        Command::FailStart { identity, reason } if is_human => {
            value(store.fail_start(request.id, identity, reason)?)
        }
        Command::Grant { identity } if is_human => value(store.grant(identity)?),
        Command::RegisterLeader { .. }
        | Command::RememberLaunch { .. }
        | Command::RegisterDefaultArea { .. }
        | Command::Grant { .. }
        | Command::AttachSession { .. }
        | Command::ConfirmTerminalExit { .. }
        | Command::RestartLeader { .. }
        | Command::AbortPreparedStart { .. }
        | Command::StartUncertain { .. }
        | Command::FailStart { .. } => Err(denied("this operation requires a human client")),
        Command::ReadMessage { message_id } => value(store.read_message(identity()?, message_id)?),
        Command::Mailbox { after, limit } => value(store.mailbox(identity()?, after, limit)?),
        Command::ClaimDelivery { message_id } => {
            value(store.claim_delivery(request.id, identity()?, message_id)?)
        }
        Command::RetryDelivery { message_id } => {
            value(store.retry_delivery(request.id, identity()?, message_id)?)
        }
        Command::AcceptDelivery { message_id } => {
            value(store.accept_delivery(request.id, message_id, identity()?)?)
        }
        Command::Context => value(store.context(identity()?)?),
        Command::UiPage {
            table,
            after,
            epoch,
        } if is_human => value(store.ui_page(table, after, epoch)?),
        Command::UiDetail {
            table,
            id,
            offset,
            epoch,
        } if is_human => value(store.ui_detail(table, id, offset, epoch)?),
        Command::UiPage { .. } | Command::UiDetail { .. } => {
            Err(denied("this operation requires a human client"))
        }
        Command::Snapshot => {
            let mut snapshot = store.snapshot()?;
            if !is_human {
                let identity = identity()?;
                let run = scope(&snapshot, identity)?;
                snapshot
                    .sessions
                    .retain(|s| s.binding.run_id() == Some(run));
                snapshot.runs.retain(|r| r.id == run);
                snapshot.tasks.retain(|t| t.run_id == run);
                snapshot.reports.retain(|r| r.run_id == run);
                snapshot.completions.retain(|r| r.run_id == run);
                snapshot.completion_requests.retain(|r| r.run_id == run);
                snapshot
                    .candidates
                    .retain(|candidate| candidate.plan.run_id == run);
                snapshot.candidate_checks.retain(|check| {
                    snapshot
                        .candidates
                        .iter()
                        .any(|candidate| candidate.plan.id == check.candidate_id)
                });
                snapshot.coordination.retain(|notice| {
                    notice
                        .participants
                        .iter()
                        .any(|participant| participant.run_id == run)
                });
                snapshot.coordination_replies.retain(|reply| {
                    snapshot
                        .coordination
                        .iter()
                        .any(|notice| notice.intent.id == reply.notice_id)
                });
                snapshot.run_work_areas.retain(|area| area.run_id == run);
                snapshot.work_areas.retain(|area| {
                    snapshot
                        .run_work_areas
                        .iter()
                        .any(|association| association.work_area_id == area.id)
                        || snapshot
                            .coordination
                            .iter()
                            .any(|notice| notice.intent.work_area_id == area.id)
                });
                snapshot
                    .attempts
                    .retain(|a| snapshot.tasks.iter().any(|t| t.id == a.task_id));
                snapshot
                    .messages
                    .retain(|m| m.run_id == run && (m.from == identity.id || m.to == identity.id));
                snapshot
                    .deliveries
                    .retain(|d| snapshot.messages.iter().any(|m| m.id == d.message_id));
            }
            value(snapshot)
        }
        Command::Events { after, limit } => {
            let mut events = store.events_after(after, limit)?;
            let next_cursor = events.last().map_or(after, |event| event.cursor);
            if !is_human {
                let run = scope(&store.snapshot()?, identity()?)?;
                // Message events contain private payloads; clients use their scoped mailbox.
                events.retain(|event| {
                    event.run_id == Some(run)
                        && !event.kind.starts_with("message_")
                        && !event.kind.starts_with("delivery_")
                });
            }
            value(EventPage {
                next_cursor,
                events,
            })
        }
        Command::OpenRun { id, brief } => {
            value(store.open_run(request.id, identity()?, id, brief)?)
        }
        Command::UpdateRun {
            run_id,
            expected_revision,
            brief,
            affected,
        } => value(store.update_run(
            request.id,
            identity()?,
            run_id,
            expected_revision,
            brief,
            affected,
        )?),
        Command::CancelRun { run_id } => {
            value(store.cancel_run(request.id, identity()?, run_id)?)
        }
        Command::Candidate { id } => {
            let candidate = store.candidate(id)?;
            if !is_human
                && store.context(identity()?)?.session.binding.run_id()
                    != Some(candidate.plan.run_id)
            {
                return Err(denied("the candidate belongs to another run"));
            }
            value(candidate.summary())
        }
        Command::ProposeCandidate {
            expected_revision,
            plan,
        } => value(
            store
                .propose_candidate(request.id, identity()?, expected_revision, plan)?
                .map(|candidate| candidate.summary()),
        ),
        Command::AnnounceChange { intent } => {
            value(store.announce_change(request.id, identity()?, intent)?)
        }
        Command::DiscussChange { reply } => {
            value(store.discuss_change(request.id, identity()?, reply)?)
        }
        Command::FinishChange { notice_id, outcome } => {
            value(store.finish_change(request.id, identity()?, notice_id, outcome)?)
        }
        Command::RequestChangeCompletion {
            run_id,
            expected_revision,
            candidate_id,
            summary,
        } => value(store.request_change_completion(
            request.id,
            identity()?,
            run_id,
            expected_revision,
            candidate_id,
            summary,
        )?),
        Command::CompleteRun {
            run_id,
            expected_revision,
            summary,
        } => value(store.complete_run(
            request.id,
            identity()?,
            run_id,
            expected_revision,
            summary,
        )?),
        Command::PutTask { plan } => value(store.put_task(request.id, identity()?, plan)?),
        Command::QueueTask { version } => {
            value(store.queue_task(request.id, identity()?, version)?)
        }
        Command::ReportTask { report } => {
            value(store.report_task(request.id, identity()?, report)?)
        }
        Command::CancelTask { version } => {
            value(store.cancel_task(request.id, identity()?, version)?)
        }
        Command::SendMessage { message } => {
            value(store.send_message(request.id, identity()?, message)?)
        }
        Command::Acknowledge { message_id } => {
            value(store.acknowledge_message(request.id, identity()?, message_id)?)
        }
    }
}

fn scope(snapshot: &ControlSnapshot, actor: SessionRef) -> Result<RunId> {
    snapshot
        .sessions
        .iter()
        .find(|s| s.reference() == actor)
        .and_then(|s| s.binding.run_id())
        .ok_or_else(|| denied("the session has no assigned run"))
}
fn value(value: impl Serialize) -> Result<Value> {
    serde_json::to_value(value).map_err(storage)
}
fn invalid(message: impl Into<String>) -> ControlError {
    ControlError::new(ErrorCode::InvalidRequest, message)
}
fn denied(message: impl Into<String>) -> ControlError {
    ControlError::new(ErrorCode::OwnershipMismatch, message)
}
fn storage(error: impl std::fmt::Display) -> ControlError {
    ControlError::new(ErrorCode::StorageFailure, error.to_string())
}
