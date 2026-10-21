//! Bounded agent-facing text; complete immutable messages stay in the store.
use adroit_model::{AgentMessage, MessageKind};
use serde_json::{Value, json};

use super::store::ControlError;

const MAX_INLINE: usize = 2048;

/// UUID fallback for integrations without a recipient-local alias store.
pub fn encode(message: &AgentMessage) -> Result<String, ControlError> {
    let value = json!(message);
    compact(message, &value, |_, value| Ok(value.into()))
}

pub(super) fn compact(
    message: &AgentMessage,
    value: &Value,
    mut alias: impl FnMut(&str, &str) -> Result<String, ControlError>,
) -> Result<String, ControlError> {
    let kind = match message.kind {
        MessageKind::Note => "n",
        MessageKind::Question => "q",
        MessageKind::Answer => "a",
        MessageKind::Report => "r",
        MessageKind::Cancel => "c",
    };
    let text = |key: &str| value[key].as_str().unwrap_or("-");
    let mut header = format!("{kind} {} {}", text("id"), text("from"));
    if let Some(revision) = message.revision {
        header.push_str(&format!(" {}.{revision}", text("task_id")));
    }
    let report = if message.kind == MessageKind::Report {
        serde_json::from_str::<Value>(&message.body).ok()
    } else {
        None
    };
    let result = report
        .as_ref()
        .map(|report| report.get("result").unwrap_or(report));
    if let Some(status) = result.and_then(|result| result["status"].as_str()) {
        header.push_str(&format!(" {status}"));
    }
    if message.reply_to.is_some() {
        header.push_str(&format!(" re={}", text("reply_to")));
    }
    if message.blocks_work {
        header.push_str(" !=blocks");
    }
    let mut body = String::new();
    if let Some(result) = result.filter(|result| result["summary"].is_string()) {
        body.push_str(result["summary"].as_str().unwrap());
        if let Some(unresolved) = result["unresolved"].as_array() {
            for issue in unresolved.iter().filter_map(Value::as_str) {
                body.push_str(&format!("\nunresolved: {issue}"));
            }
        }
        if let Some(artifact) = result.get("artifact").filter(|value| !value.is_null()) {
            body.push_str(&format!(
                "\nart {}",
                artifact["kind"].as_str().unwrap_or("artifact")
            ));
            if let Some(backend) = artifact["backend"].as_str() {
                body.push_str(&format!(" {backend}"));
            }
            if let Some(revision) = artifact["revision"].as_str() {
                let reference = alias("h", revision)?;
                let short: String = revision.chars().take(12).collect();
                body.push_str(&format!(" rev={short} ref={reference}"));
            }
            if let Some(area) = artifact["work_area_id"].as_str() {
                body.push_str(&format!(" area={}", alias("a", area)?));
            }
        }
        body.push_str(&format!(
            "\nread_message {} for artifact and checks",
            text("id")
        ));
    } else {
        body.push_str(&message.body);
    }
    let mut encoded = String::new();
    for character in format!("{header}\n{body}").chars() {
        if character.is_control() && !matches!(character, '\n' | '\t') {
            encoded.push_str(&format!("\\u{:04x}", character as u32));
        } else {
            encoded.push(character);
        }
    }
    if encoded.len() > MAX_INLINE {
        let reference = format!("\nread_message {} for full body", text("id"));
        let mut boundary = MAX_INLINE.saturating_sub(reference.len());
        while !encoded.is_char_boundary(boundary) {
            boundary -= 1;
        }
        encoded.truncate(boundary);
        encoded.push_str(&reference);
    }
    Ok(encoded)
}
