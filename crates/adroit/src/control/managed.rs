//! Automatic ownership for newly created human sessions; existing sessions stay explicit.
use std::path::{Path, PathBuf};

use adroit_model::*;

use super::{protocol::Command, store::Reply};
use crate::{
    execution::{self, ControlContext, ResolvedLaunch, StartMode, StartRequest},
    settings::Settings,
};

pub struct PreparedStart {
    pub resolved: ResolvedLaunch,
    pub ownership: Option<StartOwnership>,
}

#[derive(Clone)]
pub struct StartOwnership {
    root: PathBuf,
    identity: SessionRef,
    cwd: PathBuf,
    conversation: String,
    attach_request: RequestId,
}

impl StartOwnership {
    pub fn identity(&self) -> SessionRef {
        self.identity
    }

    pub async fn failed(&self, error: &adroit_term::Error) {
        let command = match error {
            adroit_term::Error::PreparationAborted { session, .. } => Command::AbortPreparedStart {
                identity: self.identity,
                terminal_id: format!("{session:032x}"),
                reason: error.to_string(),
            },
            adroit_term::Error::NotFound(_) => Command::FailStart {
                identity: self.identity,
                reason: error.to_string(),
            },
            _ => Command::StartUncertain {
                identity: self.identity,
                reason: error.to_string(),
            },
        };
        let _ = super::app_command(&self.root, RequestId::new(), None, command).await;
    }

    pub async fn attach(&self, terminal: u128) -> adroit_term::Result<()> {
        super::app_command(
            &self.root,
            self.attach_request,
            None,
            Command::AttachSession {
                identity: self.identity,
                cwd: self.cwd.clone(),
                terminal_id: format!("{terminal:032x}"),
                conversation_id: self.conversation.clone(),
            },
        )
        .await
        .map(|_| ())
        .map_err(|error| adroit_term::Error::Protocol(error.to_string()))
    }
}

pub(crate) fn is_root(path: &Path) -> Result<bool, String> {
    for parent in path.ancestors() {
        for marker in [".jj", ".git"] {
            match std::fs::symlink_metadata(parent.join(marker)) {
                Ok(_) => return Ok(parent == path),
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                Err(error) => return Err(format!("cannot inspect the session root: {error}")),
            }
        }
    }
    Ok(true)
}

pub async fn prepare(
    root: &Path,
    settings: &Settings,
    mut request: StartRequest,
    program: &Path,
) -> Result<PreparedStart, String> {
    request.cwd = std::fs::canonicalize(&request.cwd).map_err(|error| error.to_string())?;
    if request.mode == StartMode::Standalone {
        if request.binding.is_some() || request.control.is_some() {
            return Err("a standalone session cannot carry a controller assignment".into());
        }
        return execution::resolve(settings, request, program).map(|resolved| PreparedStart {
            resolved,
            ownership: None,
        });
    }
    if request.binding.is_some() {
        request.developer_instructions = crate::agents::adapter(request.agent)
            .developer_instructions(settings.clone(), request.cwd.clone())
            .await?;
        return execution::resolve(settings, request, program).map(|resolved| PreparedStart {
            resolved,
            ownership: None,
        });
    }
    if request.mode == StartMode::Automatic && !is_root(&request.cwd)? {
        return execution::resolve(settings, request, program).map(|resolved| PreparedStart {
            resolved,
            ownership: None,
        });
    }
    let id = LogicalSessionId::new();
    let session = super::app_command(
        root,
        RequestId::new(),
        None,
        Command::RegisterLeader {
            id,
            agent: request.agent,
            cwd: request.cwd.clone(),
            permissions: request.permissions.clone(),
        },
    )
    .await
    .map_err(|error| error.to_string())?;
    let session: Reply<ManagedSession> =
        serde_json::from_value(session).map_err(|error| error.to_string())?;
    let identity = session.value.reference();
    let run_id = RunId::new();
    let result = async {
        super::app_command(
            root,
            RequestId::new(),
            Some(identity),
            Command::OpenRun {
                id: run_id,
                brief: RunBrief {
                    title: request.name.clone(),
                    goal: request.prompt.clone(),
                    ..Default::default()
                },
            },
        )
        .await
        .map_err(|error| error.to_string())?;
        let area = crate::workareas::existing(
            WorkAreaId::new(),
            request.cwd.clone(),
            "Default working area".into(),
        )
        .await?;
        super::app_command(
            root,
            RequestId::new(),
            None,
            Command::RegisterDefaultArea {
                identity,
                run_id,
                area,
            },
        )
        .await
        .map_err(|error| error.to_string())?;
        let token = super::app_command(root, RequestId::new(), None, Command::Grant { identity })
            .await
            .map_err(|error| error.to_string())?;
        let token: String = serde_json::from_value(token).map_err(|error| error.to_string())?;
        request.binding = Some(SessionBinding::leader(id, Some(run_id)));
        request.control = Some(ControlContext::new(identity, root.into(), token));
        let target = execution::select_target(
            settings,
            request.agent,
            request.model.clone(),
            request.permissions.clone(),
        )?;
        let mut runtime = super::runtime::RuntimeLaunch::new(settings.clone()).with_target(target);
        runtime.developer_instructions = crate::agents::adapter(request.agent)
            .developer_instructions(runtime.settings.clone(), request.cwd.clone())
            .await?;
        request.developer_instructions = runtime.developer_instructions.clone();
        super::app_command(
            root,
            RequestId::new(),
            None,
            Command::RememberLaunch {
                identity,
                runtime: Box::new(runtime.clone()),
            },
        )
        .await
        .map_err(|error| error.to_string())?;
        execution::resolve(&runtime.settings, request, program)
    }
    .await;
    match result {
        Ok(resolved) => {
            let ownership = StartOwnership {
                root: root.to_path_buf(),
                identity,
                cwd: resolved.spawn.cwd.clone(),
                conversation: resolved.conversation.clone(),
                attach_request: RequestId::new(),
            };
            Ok(PreparedStart {
                resolved,
                ownership: Some(ownership),
            })
        }
        Err(error) => {
            let _ = super::app_command(
                root,
                RequestId::new(),
                None,
                Command::FailStart {
                    identity,
                    reason: error.clone(),
                },
            )
            .await;
            Err(error)
        }
    }
}
