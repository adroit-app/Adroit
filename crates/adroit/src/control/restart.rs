//! Explicit handoff of a saved request to a fresh Leader conversation.
use std::path::Path;

use adroit_model::*;
use adroit_term::Daemon;

use super::{
    actor::Actor,
    store::{ControlError, ErrorCode},
};
use crate::{execution, settings::Settings};

pub(super) async fn start(
    actor: &Actor,
    daemon: &Daemon,
    root: &Path,
    program: &Path,
    session: ManagedSession,
    handoff: Option<String>,
) -> Result<(), ControlError> {
    let identity = session.reference();
    let runtime = actor
        .call(move |store| store.runtime(identity))
        .await?
        .ok_or_else(|| failed("the Leader's saved routing settings are missing"))?;
    let credential = actor.call(move |store| store.grant(identity)).await?;
    let cwd = session
        .cwd
        .ok_or_else(|| failed("the Leader's working directory is missing"))?;
    let target = runtime.target.as_ref();
    let resolved = execution::resolve(&runtime.settings, execution::StartRequest {
        agent: session.agent,
        model: target.and_then(|target| target.requested_model.clone()),
        permissions: session.permissions,
        developer_instructions: runtime.developer_instructions.clone(),
        name: format!("leader-{}", identity.id),
        cwd: cwd.clone(),
        prompt: handoff.unwrap_or_else(|| "This is an explicit restart of the Leader for an existing request in a fresh conversation. The earlier native conversation has ended. Read Adroit context, snapshot, and mailbox before taking action. Keep the existing run, task, Worker and report identities. Review unanswered questions and uncertain deliveries; do not launch replacement Workers just because this conversation is new. Continue orchestration and report the saved request's progress to the user.".into()),
        binding: Some(session.binding),
        control: Some(execution::ControlContext::new(identity, root.into(), credential)),
        env: crate::daemon::terminal_env(),
        ..Default::default()
    }, program).map_err(failed)?;
    let palette =
        crate::themes::palette(&runtime.settings.theme, &Settings::themes_dir()).map_err(failed)?;
    let conversation = resolved.conversation;
    let owner = actor.clone();
    let started = execution::spawn_headless_prepared(
        daemon,
        resolved.spawn,
        &palette,
        move |terminal| async move {
            owner
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
    match started {
        Ok(_) => Ok(()),
        Err(adroit_term::Error::NotFound(program)) => Err(failed(format!(
            "the Leader program was not found: {program}"
        ))),
        Err(adroit_term::Error::PreparationAborted { session, reason }) => {
            actor
                .call(move |store| {
                    store
                        .abort_prepared_start(
                            RequestId::new(),
                            identity,
                            format!("{session:032x}"),
                            reason,
                        )
                        .map(|_| ())
                })
                .await
        }
        Err(error) => {
            actor
                .call(move |store| {
                    store
                        .start_uncertain(RequestId::new(), identity, error.to_string())
                        .map(|_| ())
                })
                .await
        }
    }
}

fn failed(error: impl std::fmt::Display) -> ControlError {
    ControlError::new(ErrorCode::StorageFailure, error.to_string())
}
