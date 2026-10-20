//! Read-only file evidence for the credential's current Worker assignment.
use super::{
    client::Client,
    protocol::Command,
    store::{ControlError, ErrorCode},
};
use adroit_model::*;

type Result<T> = std::result::Result<T, ControlError>;

pub async fn capture(client: &Client, paths: Vec<String>) -> Result<String> {
    let context: AgentContext =
        serde_json::from_value(client.request(RequestId::new(), Command::Context).await?)
            .map_err(failed)?;
    if context.session.binding.role() != SessionRole::Worker {
        return Err(ControlError::new(
            ErrorCode::OwnershipMismatch,
            "file evidence is performed by an assigned Worker",
        ));
    }
    let task = context
        .task
        .ok_or_else(|| failed("the Worker task is missing"))?;
    let attempt = context
        .attempt
        .ok_or_else(|| failed("the Worker attempt is missing"))?;
    if task.current_attempt != Some(attempt.id) || task.revision != attempt.task_revision {
        return Err(ControlError::new(
            ErrorCode::RevisionConflict,
            "file evidence needs the current task assignment",
        ));
    }
    if context.session.state != ManagedSessionState::Running
        || attempt.state != AttemptState::Running
    {
        return Err(ControlError::new(
            ErrorCode::Conflict,
            "file evidence needs a running Worker",
        ));
    }
    let area = context
        .work_area
        .filter(|area| area.state == WorkAreaState::Ready)
        .ok_or_else(|| failed("the Worker working area is not ready"))?;
    tokio::task::spawn_blocking(move || {
        crate::candidates::capture_files(&area.path, &paths)
            .map(|evidence| evidence.fingerprint)
            .map_err(failed)
    })
    .await
    .map_err(failed)?
}

fn failed(error: impl std::fmt::Display) -> ControlError {
    ControlError::new(ErrorCode::StorageFailure, error.to_string())
}

pub fn cli() -> i32 {
    let request = RequestId::new();
    let result = (|| {
        use std::io::Read;
        let token = std::env::var("ADROIT_CONTROL_TOKEN")
            .map_err(|_| failed("file evidence requires an assigned Worker credential"))?;
        let mut input = Vec::new();
        std::io::stdin()
            .take(64 * 1024 + 1)
            .read_to_end(&mut input)
            .map_err(failed)?;
        if input.len() > 64 * 1024 {
            return Err(failed("the file manifest exceeds 64 KiB"));
        }
        let paths = serde_json::from_slice(&input).map_err(failed)?;
        let client = Client::session(&super::session_root(), token);
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .map_err(failed)?;
        runtime.block_on(capture(&client, paths))
    })();
    let exit = if result.is_ok() { 0 } else { 1 };
    println!(
        "{}",
        serde_json::json!({"request_id":request,"result":result})
    );
    exit
}
