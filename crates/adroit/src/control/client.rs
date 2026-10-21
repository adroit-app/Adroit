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
    human: bool,
}

impl Client {
    pub fn human(root: &Path) -> Result<Self, ControlError> {
        let credential = std::fs::read_to_string(root.join("human.token")).map_err(transport)?;
        let mut client = Self::session(root, credential);
        client.human = true;
        Ok(client)
    }

    pub fn session(root: &Path, credential: String) -> Self {
        Self {
            socket: root.join("control.sock"),
            credential,
            actor: None,
            human: false,
        }
    }

    pub fn as_actor(mut self, actor: SessionRef) -> Self {
        self.actor = Some(actor);
        self
    }

    pub async fn request(&self, id: RequestId, command: Command) -> Result<Value, ControlError> {
        let ui_read = matches!(command, Command::UiPage { .. } | Command::UiDetail { .. });
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
            // Old daemons cannot decode a new enum variant, so have no request ID.
            if ui_read
                && response.version == VERSION
                && response.id.is_none()
                && response.result.as_ref().is_err_and(|error| {
                    error.code == ErrorCode::InvalidRequest
                        && error.message == "invalid control request"
                })
            {
                return Err(ControlError::new(
                    ErrorCode::InvalidRequest,
                    "unsupported UI projection",
                ));
            }
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
    /// Only the human UI uses the legacy read-only compatibility path.
    pub async fn ui_snapshot(&self) -> Result<adroit_model::ControlSnapshot, ControlError> {
        self.require_human_ui()?;
        for _ in 0..3 {
            match self.ui_snapshot_pages().await {
                Err(error) if legacy_ui_error(&error) => {
                    let root = self.socket.parent().expect("control root").to_path_buf();
                    let credential = self.credential.clone();
                    return tokio::task::spawn_blocking(move || {
                        super::store::ui::read_only_snapshot(&root, &credential)
                    })
                    .await
                    .map_err(transport)?;
                }
                Err(error) if error.code == ErrorCode::RevisionConflict => continue,
                result => return result,
            }
        }
        Err(ControlError::new(
            ErrorCode::RevisionConflict,
            "controller changed during UI refresh; retry",
        ))
    }

    fn require_human_ui(&self) -> Result<(), ControlError> {
        if !self.human || self.actor.is_some() {
            return Err(ControlError::new(
                ErrorCode::OwnershipMismatch,
                "UI reads require a human client without an actor",
            ));
        }
        Ok(())
    }

    async fn ui_snapshot_pages(&self) -> Result<adroit_model::ControlSnapshot, ControlError> {
        let mut snapshot =
            serde_json::to_value(adroit_model::ControlSnapshot::default()).map_err(transport)?;
        let mut epoch = None;
        for table in UiTable::ALL {
            let mut after = 0;
            loop {
                let page: UiPage = serde_json::from_value(
                    self.request(
                        RequestId::new(),
                        Command::UiPage {
                            table,
                            after,
                            epoch,
                        },
                    )
                    .await?,
                )
                .map_err(transport)?;
                if epoch.is_some_and(|epoch| epoch != page.epoch) {
                    return Err(transport("inconsistent UI page epoch"));
                }
                epoch = Some(page.epoch);
                snapshot["event_cursor"] = page.epoch.into();
                snapshot[table.field()]
                    .as_array_mut()
                    .expect("snapshot array")
                    .extend(page.rows);
                match page.next_cursor {
                    Some(next) if next > after => after = next,
                    Some(_) => return Err(transport("UI cursor did not advance")),
                    None => break,
                }
            }
        }
        serde_json::from_value(snapshot).map_err(transport)
    }

    pub async fn ui_detail(&self, table: UiTable, id: String) -> Result<String, ControlError> {
        self.require_human_ui()?;
        let mut text = String::new();
        let mut offset = 0;
        let mut epoch = None;
        loop {
            let value = self
                .request(
                    RequestId::new(),
                    Command::UiDetail {
                        table,
                        id: id.clone(),
                        offset,
                        epoch,
                    },
                )
                .await;
            let value = match value {
                Err(error) if legacy_ui_error(&error) => {
                    let root = self.socket.parent().expect("control root").to_path_buf();
                    let credential = self.credential.clone();
                    return tokio::task::spawn_blocking(move || {
                        super::store::ui::read_only_detail(&root, &credential, table, id)
                    })
                    .await
                    .map_err(transport)?;
                }
                result => result?,
            };
            let page: UiDetailPage = serde_json::from_value(value).map_err(transport)?;
            if epoch.is_some_and(|epoch| epoch != page.epoch) {
                return Err(transport("inconsistent UI detail epoch"));
            }
            epoch = Some(page.epoch);
            text.push_str(&page.text);
            match page.next_offset {
                Some(next) if next > offset => offset = next,
                Some(_) => return Err(transport("UI detail offset did not advance")),
                None => return Ok(text),
            }
        }
    }
}

fn legacy_ui_error(error: &ControlError) -> bool {
    error.code == ErrorCode::InvalidRequest
        && matches!(
            error.message.as_str(),
            "unsupported UI projection" | "the response exceeds the control frame limit"
        )
}

fn transport(error: impl std::fmt::Display) -> ControlError {
    ControlError::new(
        ErrorCode::StorageFailure,
        format!("control transport: {error}"),
    )
}
