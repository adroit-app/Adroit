//! Automatic ownership for newly created human sessions; existing sessions stay explicit.
use std::path::{Path, PathBuf};

use adroit_model::*;

use super::{client::Client, protocol::Command, store::Reply};
use crate::{
    execution::{self, ControlContext, ResolvedLaunch, StartRequest},
    settings::Settings,
};

pub struct PreparedStart {
    pub resolved: ResolvedLaunch,
    pub ownership: Option<StartOwnership>,
}

#[derive(Clone)]
pub struct StartOwnership {
    client: Client,
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
            adroit_term::Error::NotFound(_) => Command::FailStart {
                identity: self.identity,
                reason: error.to_string(),
            },
            _ => Command::StartUncertain {
                identity: self.identity,
                reason: error.to_string(),
            },
        };
        let _ = self.client.request(RequestId::new(), command).await;
    }

    pub async fn attach(&self, terminal: u128) -> adroit_term::Result<()> {
        self.client
            .request(
                self.attach_request,
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

fn is_root(path: &Path) -> Result<bool, String> {
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
    if request.binding.is_some() || !is_root(&request.cwd)? {
        return execution::resolve(settings, request, program).map(|resolved| PreparedStart {
            resolved,
            ownership: None,
        });
    }
    let client = Client::human(root).map_err(|error| error.to_string())?;
    let id = LogicalSessionId::new();
    let session = client
        .request(
            RequestId::new(),
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
        client
            .clone()
            .as_actor(identity)
            .request(
                RequestId::new(),
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
        client
            .request(
                RequestId::new(),
                Command::RegisterDefaultArea {
                    identity,
                    run_id,
                    area,
                },
            )
            .await
            .map_err(|error| error.to_string())?;
        let token = client
            .request(RequestId::new(), Command::Grant { identity })
            .await
            .map_err(|error| error.to_string())?;
        let token: String = serde_json::from_value(token).map_err(|error| error.to_string())?;
        request.binding = Some(SessionBinding::leader(id, Some(run_id)));
        request.control = Some(ControlContext::new(identity, root.into(), token));
        let runtime = super::runtime::RuntimeLaunch::new(settings.clone());
        client
            .request(
                RequestId::new(),
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
                client,
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
            let _ = client
                .request(
                    RequestId::new(),
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
