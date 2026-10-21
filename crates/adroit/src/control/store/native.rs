//! Recipient-local aliases are presentation state, never orchestration identity.
use super::*;

fn alias(
    connection: &Connection,
    recipient: LogicalSessionId,
    kind: &str,
    value: &str,
) -> Result<String> {
    connection.execute(
        "INSERT OR IGNORE INTO native_aliases(recipient,kind,number,value) SELECT ?1,?2,COALESCE(MAX(number),0)+1,?3 FROM native_aliases WHERE recipient=?1 AND kind=?2",
        params![recipient.to_string(), kind, value],
    )?;
    let number: i64 = connection.query_row(
        "SELECT number FROM native_aliases WHERE recipient=?1 AND kind=?2 AND value=?3",
        params![recipient.to_string(), kind, value],
        |row| row.get(0),
    )?;
    Ok(format!("{kind}{number}"))
}

fn resolve(
    connection: &Connection,
    recipient: LogicalSessionId,
    kind: &str,
    value: &str,
) -> Result<String> {
    if value.parse::<MessageId>().is_ok() || (kind == "h" && is_hash(value)) {
        return Ok(value.into());
    }
    let number = value
        .strip_prefix(kind)
        .and_then(|suffix| suffix.parse::<i64>().ok())
        .ok_or_else(|| invalid(format!("expected a {kind} alias or a full identifier")))?;
    connection
        .query_row(
            "SELECT value FROM native_aliases WHERE recipient=?1 AND kind=?2 AND number=?3",
            params![recipient.to_string(), kind, number],
            |row| row.get(0),
        )
        .optional()?
        .ok_or_else(|| {
            ControlError::new(
                ErrorCode::NotFound,
                format!("unknown alias {value} for this session"),
            )
        })
}

fn is_hash(value: &str) -> bool {
    matches!(value.len(), 40 | 64) && value.bytes().all(|byte| byte.is_ascii_hexdigit())
}

fn identity_kind(key: &str) -> Option<&'static str> {
    match key {
        "logical_id" | "session_id" | "leader_id" | "from" | "to" | "recipient" => Some("w"),
        "run_id" => Some("r"),
        "task_id" | "source_task_id" | "dependencies" => Some("t"),
        "attempt_id" | "current_attempt" => Some("e"),
        "message_id" | "report_id" | "reply_to" | "source_report" | "reports" => Some("m"),
        "work_area_id" => Some("a"),
        "candidate_id" | "candidate" => Some("c"),
        "notice_id" => Some("n"),
        _ => None,
    }
}

fn record_kind(fields: &serde_json::Map<String, Value>) -> Option<&'static str> {
    if fields.contains_key("generation") {
        Some("w")
    } else if fields.contains_key("body") {
        Some("m")
    } else if fields.contains_key("operation") && fields.contains_key("scope") {
        Some("n")
    } else if fields.contains_key("source") && fields.contains_key("attempt_id") {
        Some("c")
    } else if fields.contains_key("backend") && fields.contains_key("path") {
        Some("a")
    } else if fields.contains_key("task_revision") && fields.contains_key("session_id") {
        Some("e")
    } else if fields.contains_key("brief") && fields.contains_key("revision") {
        Some("t")
    } else if fields.contains_key("leader_id") && fields.contains_key("plan_revision") {
        Some("r")
    } else {
        None
    }
}

fn project(
    connection: &Connection,
    recipient: LogicalSessionId,
    value: &mut Value,
    key: &str,
) -> Result<()> {
    // Evidence and user prose may contain identifier-shaped text with unrelated meaning.
    if matches!(
        key,
        "data" | "body" | "brief" | "command" | "manifest" | "files" | "scope" | "permissions"
    ) {
        return Ok(());
    }
    match value {
        Value::String(text) => {
            if text.parse::<MessageId>().is_ok() {
                let kind = identity_kind(key);
                if let Some(kind) = kind {
                    *text = alias(connection, recipient, kind, text)?;
                }
            } else if matches!(key, "revision" | "reviewed_revision" | "base" | "candidate")
                && is_hash(text)
            {
                *text = alias(connection, recipient, "h", text)?;
            }
        }
        Value::Array(values) => {
            for value in values {
                project(connection, recipient, value, key)?;
            }
        }
        Value::Object(fields) => {
            let kind = record_kind(fields);
            for (key, value) in fields {
                if key == "id"
                    && let (Some(kind), Some(text)) = (kind, value.as_str())
                {
                    *value = json!(alias(connection, recipient, kind, text)?);
                } else {
                    project(connection, recipient, value, key)?;
                }
            }
        }
        _ => {}
    }
    Ok(())
}

impl Store {
    pub fn resolve_native_ids(&self, identity: SessionRef, ids: Vec<String>) -> Result<Value> {
        reference(&self.connection, identity)?;
        if ids.len() > 64 {
            return Err(invalid("resolve at most 64 IDs at a time"));
        }
        let mut resolved = serde_json::Map::new();
        for id in ids {
            let kind = id.get(..1).filter(|kind| {
                matches!(*kind, "w" | "r" | "t" | "e" | "m" | "a" | "c" | "n" | "h")
            });
            let full = if id.parse::<MessageId>().is_ok() {
                id.clone()
            } else {
                resolve(
                    &self.connection,
                    identity.id,
                    kind.ok_or_else(|| invalid("unknown alias kind"))?,
                    &id,
                )?
            };
            resolved.insert(id, json!(full));
        }
        Ok(Value::Object(resolved))
    }

    pub fn native_arguments(
        &mut self,
        identity: SessionRef,
        name: &str,
        arguments: Value,
    ) -> Result<Value> {
        reference(&self.connection, identity)?;
        super::super::tools::resolve_aliases(name, arguments, |kind, value| {
            resolve(&self.connection, identity.id, kind, value)
        })
    }

    pub fn native_output(
        &mut self,
        identity: SessionRef,
        name: &str,
        mut value: Value,
    ) -> Result<String> {
        let transaction = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        reference(&transaction, identity)?;
        if name != "read_message" {
            previews(&mut value);
        }
        project(&transaction, identity.id, &mut value, "")?;
        let text = if let Some(record) = value.get("value") {
            let subject = record
                .get("report")
                .or_else(|| record.get("intent"))
                .or_else(|| record.get("plan"))
                .unwrap_or(record);
            let id = subject
                .get("id")
                .or_else(|| subject.get("message_id"))
                .or_else(|| subject.get("run_id"))
                .and_then(Value::as_str);
            let mut text = String::from("ok");
            if let Some(id) = id {
                text.push_str(&format!(" {id}"));
            }
            if let Some(revision) = record
                .get("revision")
                .or_else(|| record.get("plan_revision"))
                .and_then(Value::as_u64)
            {
                text.push_str(&format!(".{revision}"));
            }
            for key in ["state", "applied"] {
                if let Some(value) = record.get(key) {
                    text.push_str(&format!(
                        " {key}={}",
                        value
                            .as_str()
                            .map(str::to_owned)
                            .unwrap_or_else(|| value.to_string())
                    ));
                }
            }
            text
        } else {
            // read_message is the explicit detail escape hatch, including the full report body.
            if name != "read_message" {
                remove_nulls(&mut value);
            }
            encode(&value)?
        };
        transaction.commit()?;
        Ok(text)
    }

    pub fn native_envelope(&mut self, identity: SessionRef, id: MessageId) -> Result<String> {
        let message = self.read_message(identity, id)?;
        let transaction = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let mut value =
            serde_json::to_value(&message).map_err(|error| invalid(error.to_string()))?;
        project(&transaction, identity.id, &mut value, "")?;
        let text = crate::control::envelope::compact(&message, &value, |kind, value| {
            alias(&transaction, identity.id, kind, value)
        })?;
        transaction.commit()?;
        Ok(text)
    }
}

fn previews(value: &mut Value) {
    match value {
        Value::Object(fields) => {
            if let Some(id) = fields
                .get("id")
                .or_else(|| fields.get("message_id"))
                .cloned()
            {
                if let Some(Value::String(body)) = fields.get("body") {
                    if fields.get("kind").and_then(Value::as_str) == Some("report")
                        && let Ok(mut report) = serde_json::from_str::<Value>(body)
                    {
                        if let Some(result) = report.get_mut("result") {
                            report = result.take();
                        }
                        if let Some(result) = report
                            .as_object_mut()
                            .filter(|result| result.contains_key("summary"))
                        {
                            result.retain(|key, _| {
                                matches!(key.as_str(), "status" | "summary" | "unresolved")
                            });
                            fields.insert("body".into(), report);
                            fields.insert("details".into(), json!({"message_id":id}));
                        }
                    } else if body.len() > 1024 {
                        let mut boundary = 1024;
                        while !body.is_char_boundary(boundary) {
                            boundary -= 1;
                        }
                        let body = body[..boundary].to_owned();
                        fields.insert("body".into(), json!(body));
                        fields.insert("details".into(), json!({"message_id":id}));
                    }
                }
                if let Some(result) = fields.get_mut("result").and_then(Value::as_object_mut) {
                    result.retain(|key, _| {
                        matches!(key.as_str(), "status" | "summary" | "unresolved")
                    });
                    fields.insert("details".into(), json!({"message_id":id}));
                }
            }
            for (key, value) in fields {
                if !matches!(
                    key.as_str(),
                    "data" | "brief" | "body" | "artifact" | "checks"
                ) {
                    previews(value);
                }
            }
        }
        Value::Array(values) => {
            for value in values {
                previews(value);
            }
        }
        _ => {}
    }
}

fn remove_nulls(value: &mut Value) {
    match value {
        Value::Object(fields) => {
            fields.retain(|_, value| !value.is_null());
            for (key, value) in fields {
                if !matches!(key.as_str(), "data" | "brief" | "body") {
                    remove_nulls(value);
                }
            }
        }
        Value::Array(values) => {
            for value in values {
                remove_nulls(value);
            }
        }
        _ => {}
    }
}
