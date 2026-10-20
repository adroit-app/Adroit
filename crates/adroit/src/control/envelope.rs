//! Bounded native notifications refer to durable bodies when inline data is large.
use adroit_model::AgentMessage;
use serde_json::json;

use super::store::{ControlError, ErrorCode};

const MAX_INLINE: usize = 16 * 1024;

pub fn encode(message: &AgentMessage) -> Result<String, ControlError> {
    let inline = serde_json::to_string(message).map_err(error)?;
    if inline.len() <= MAX_INLINE {
        return Ok(inline);
    }
    serde_json::to_string(&json!({
        "type":"adroit_message_reference","version":1,
        "message_id":message.id,"run_id":message.run_id,"from":message.from,"to":message.to,
        "task_id":message.task_id,"attempt_id":message.attempt_id,"revision":message.revision,"kind":message.kind
    })).map_err(error)
}

fn error(error: serde_json::Error) -> ControlError {
    ControlError::new(ErrorCode::InvalidRequest, error.to_string())
}
