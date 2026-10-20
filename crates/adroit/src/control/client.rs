//! Bounded requests to the daemon, independent of any window.
use std::path::{Path, PathBuf};

use adroit_model::{RequestId, SessionRef};
use serde_json::Value;
use tokio::io::{AsyncWriteExt, BufReader};

use super::{
    protocol::*,
    service::read_frame,
    store::{ControlError, ErrorCode},
};

#[derive(Clone)]
pub struct Client {
    socket: PathBuf,
    credential: String,
    actor: Option<SessionRef>,
}

impl Client {
    pub fn human(root: &Path) -> Result<Self, ControlError> {
        let credential = std::fs::read_to_string(root.join("human.token")).map_err(transport)?;
        Ok(Self::session(root, credential))
    }

    pub fn session(root: &Path, credential: String) -> Self {
        Self {
            socket: root.join("control.sock"),
            credential,
            actor: None,
        }
    }

    pub fn as_actor(mut self, actor: SessionRef) -> Self {
        self.actor = Some(actor);
        self
    }

    pub async fn request(&self, id: RequestId, command: Command) -> Result<Value, ControlError> {
        tokio::time::timeout(TIMEOUT, async {
            let mut bytes = serde_json::to_vec(&Request {
                version: VERSION,
                id,
                credential: self.credential.clone(),
                actor: self.actor,
                command,
            })
            .map_err(transport)?;
            if bytes.len() >= MAX_FRAME {
                return Err(ControlError::new(
                    ErrorCode::InvalidRequest,
                    "the control request is too large",
                ));
            }
            bytes.push(b'\n');
            let mut stream = tokio::net::UnixStream::connect(&self.socket)
                .await
                .map_err(transport)?;
            stream.write_all(&bytes).await.map_err(transport)?;
            let response: Response =
                serde_json::from_slice(&read_frame(&mut BufReader::new(stream)).await?)
                    .map_err(transport)?;
            if response.version != VERSION || response.id != Some(id) {
                return Err(ControlError::new(
                    ErrorCode::InvalidRequest,
                    "the response does not match the request",
                ));
            }
            response.result
        })
        .await
        .map_err(|_| {
            ControlError::new(
                ErrorCode::StorageFailure,
                "the control request timed out; retry with the same request ID",
            )
        })?
    }
}

fn transport(error: impl std::fmt::Display) -> ControlError {
    ControlError::new(
        ErrorCode::StorageFailure,
        format!("control transport: {error}"),
    )
}
