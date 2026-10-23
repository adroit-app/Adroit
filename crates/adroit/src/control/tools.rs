//! Native tools expose one operation and its fields, without a command union.
use std::collections::{BTreeSet, VecDeque};
use std::sync::LazyLock;

use adroit_model::RequestId;
use serde_json::{Value, json};

use super::protocol::Command;

static CATALOG: LazyLock<Vec<Value>> = LazyLock::new(|| {
    let commands = serde_json::to_value(schemars::schema_for!(Command)).unwrap();
    let definitions = commands["$defs"].as_object().unwrap();
    let mut request = serde_json::to_value(schemars::schema_for!(RequestId)).unwrap();
    request.as_object_mut().unwrap().remove("$schema");
    request.as_object_mut().unwrap().remove("title");
    commands["oneOf"].as_array().unwrap().iter().map(|variant| {
        let name = variant["properties"]["op"]["const"].as_str().unwrap();
        let mut schema = if let Some(field) = wrapper(name) {
            let reference = variant["properties"][field]["$ref"].as_str().unwrap();
            definitions[reference.strip_prefix("#/$defs/").unwrap()].clone()
        } else {
            variant.clone()
        };
        let properties = schema["properties"].as_object_mut().unwrap();
        properties.remove("op");
        properties.insert("request_id".into(), request.clone());
        if name == "propose_candidate" {
            properties.insert("expected_revision".into(), variant["properties"]["expected_revision"].clone());
        }
        let required = schema["required"].as_array_mut().unwrap();
        required.retain(|field| field != "op");
        if name == "propose_candidate" { required.push(json!("expected_revision")); }
        let mut pending = VecDeque::new();
        references(&schema, &mut pending);
        let mut visited = BTreeSet::new();
        let mut reachable = serde_json::Map::new();
        while let Some(name) = pending.pop_front() {
            if visited.insert(name.clone()) {
                let definition = definitions.get(&name).expect("generated local schema reference");
                references(definition, &mut pending);
                reachable.insert(name, definition.clone());
            }
        }
        for (name, definition) in &mut reachable {
            if let Some(kind) = id_kind(name) {
                *definition = json!({"type":"string", "pattern":format!("^({kind}[1-9][0-9]*|[0-9a-fA-F]{{8}}-[0-9a-fA-F]{{4}}-[0-9a-fA-F]{{4}}-[0-9a-fA-F]{{4}}-[0-9a-fA-F]{{12}})$"), "description":"A full UUID or an alias issued to this session."});
            }
        }
        if !reachable.is_empty() { schema["$defs"] = Value::Object(reachable); }
        schema.as_object_mut().unwrap().remove("title");
        let mut tool = json!({
            "name":name,
            "description":format!("Adroit {name}: {} request_id is optional; supply it for an identical retry. IDs accept your session's short aliases or full UUIDs. Pass fields directly without command or op wrappers.", description(name)),
            "inputSchema":schema
        });
        if matches!(name, "context" | "models" | "resolve_ids" | "snapshot" | "read_message" | "mailbox" | "events" | "candidate") {
            tool["annotations"] = json!({"readOnlyHint":true,"destructiveHint":false,"idempotentHint":true,"openWorldHint":false});
        }
        tool
    }).collect()
});

pub(super) fn catalog() -> &'static [Value] {
    &CATALOG
}

pub(super) fn contains(name: &str) -> bool {
    CATALOG.iter().any(|tool| tool["name"] == name)
}

pub(super) fn decode(name: &str, arguments: Value) -> Result<(RequestId, Command), String> {
    if !contains(name) {
        return Err("unknown Adroit operation".into());
    }
    let mut arguments = arguments
        .as_object()
        .cloned()
        .ok_or("tool arguments must be an object")?;
    if arguments.contains_key("op") || arguments.contains_key("command") {
        return Err("pass the named tool's fields directly, without op or command".into());
    }
    let request = arguments
        .remove("request_id")
        .map(serde_json::from_value)
        .transpose()
        .map_err(|_| "request_id must be a UUID")?
        .unwrap_or_else(RequestId::new);
    let expected_revision = if name == "propose_candidate" {
        arguments.remove("expected_revision")
    } else {
        None
    };
    let mut command = if let Some(field) = wrapper(name) {
        json!({(field):arguments})
    } else {
        Value::Object(arguments)
    };
    command["op"] = json!(name);
    if let Some(revision) = expected_revision {
        command["expected_revision"] = revision;
    }
    let command = serde_json::from_value(command)
        .map_err(|error| format!("invalid {name} arguments: {error}"))?;
    Ok((request, command))
}

fn wrapper(name: &str) -> Option<&'static str> {
    match name {
        "put_task" | "propose_candidate" => Some("plan"),
        "queue_task" => Some("version"),
        "report_task" => Some("report"),
        "send_message" => Some("message"),
        "announce_change" => Some("intent"),
        _ => None,
    }
}

fn references(value: &Value, pending: &mut VecDeque<String>) {
    match value {
        Value::Object(map) => {
            if let Some(reference) = map
                .get("$ref")
                .and_then(Value::as_str)
                .and_then(|reference| reference.strip_prefix("#/$defs/"))
            {
                pending.push_back(reference.replace("~1", "/").replace("~0", "~"));
            }
            for value in map.values() {
                references(value, pending);
            }
        }
        Value::Array(values) => {
            for value in values {
                references(value, pending);
            }
        }
        _ => {}
    }
}

fn description(name: &str) -> &'static str {
    match name {
        "context" => "Read your current session, run, task, attempt and working area.",
        "models" => {
            "Read the models each agent's provider accepts and which agent a task without target.agent runs. A task's target.model must be one of its agent's listed ids; an agent without a list is not checked."
        }
        "resolve_ids" => {
            "Read full immutable identifiers for your aliases before passing them to another session."
        }
        "put_task" => {
            "Create or revise one task. A new task requires expected_revision null; revisions use the task's current revision. This records the plan without starting a Worker."
        }
        "queue_task" => {
            "Queue the recorded task_id and revision for asynchronous Worker startup, without waiting for completion."
        }
        "report_task" => {
            "Record the assigned Worker's structured outcome, including its task version, attempt, evidence and unresolved issues."
        }
        "acknowledge" => {
            "Optionally mark an addressed message handled. Native transport acceptance already records receipt; do not reply merely to acknowledge."
        }
        "snapshot" => {
            "Read the current request's saved state; do not poll in a Leader turn waiting for Workers."
        }
        "complete_run" => {
            "Request report-only completion after all current required task results are satisfied."
        }
        "request_change_completion" => {
            "Request asynchronous final verification of the current change candidate; end the turn while it runs."
        }
        _ => {
            "Operate within the credential's assigned request; do not infer identity from directories or session names."
        }
    }
}

pub(super) fn resolve_aliases(
    name: &str,
    mut arguments: Value,
    mut resolve: impl FnMut(&str, &str) -> Result<String, super::store::ControlError>,
) -> Result<Value, super::store::ControlError> {
    let tool = catalog()
        .iter()
        .find(|tool| tool["name"] == name)
        .ok_or_else(|| {
            super::store::ControlError::new(
                super::store::ErrorCode::InvalidRequest,
                "unknown Adroit operation",
            )
        })?;
    let schema = &tool["inputSchema"];
    walk_arguments(&mut arguments, schema, &schema["$defs"], &mut resolve)?;
    Ok(arguments)
}

fn walk_arguments(
    value: &mut Value,
    schema: &Value,
    definitions: &Value,
    resolve: &mut impl FnMut(&str, &str) -> Result<String, super::store::ControlError>,
) -> Result<(), super::store::ControlError> {
    if value.is_null() {
        return Ok(());
    }
    if let Some(name) = schema["$ref"]
        .as_str()
        .and_then(|value| value.strip_prefix("#/$defs/"))
    {
        let kind = id_kind(name);
        if let (Some(kind), Some(text)) = (kind, value.as_str()) {
            *value = json!(resolve(kind, text)?);
            return Ok(());
        }
        return walk_arguments(value, &definitions[name], definitions, resolve);
    }
    for union in ["oneOf", "anyOf", "allOf"] {
        if let Some(variants) = schema[union].as_array() {
            for variant in variants {
                let matches = variant["properties"].as_object().is_none_or(|fields| {
                    fields.iter().all(|(key, field)| {
                        field
                            .get("const")
                            .is_none_or(|constant| value.get(key) == Some(constant))
                    })
                });
                if matches {
                    walk_arguments(value, variant, definitions, resolve)?;
                }
            }
        }
    }
    if let (Some(fields), Some(properties)) =
        (value.as_object_mut(), schema["properties"].as_object())
    {
        for (key, field) in fields {
            if let Some(schema) = properties.get(key) {
                if matches!(
                    key.as_str(),
                    "revision" | "reviewed_revision" | "base" | "candidate"
                ) && let Some(text) = field.as_str()
                    && text.starts_with('h')
                    && text[1..].parse::<u64>().is_ok()
                {
                    *field = json!(resolve("h", text)?);
                } else if key == "candidate"
                    && let Some(text) = field.as_str()
                    && text.starts_with('c')
                    && text[1..].parse::<u64>().is_ok()
                {
                    *field = json!(resolve("c", text)?);
                } else {
                    walk_arguments(field, schema, definitions, resolve)?;
                }
            }
        }
    }
    if let (Some(values), Some(items)) = (value.as_array_mut(), schema.get("items")) {
        for value in values {
            walk_arguments(value, items, definitions, resolve)?;
        }
    }
    Ok(())
}

fn id_kind(name: &str) -> Option<&'static str> {
    match name {
        "LogicalSessionId" => Some("w"),
        "RunId" => Some("r"),
        "TaskId" => Some("t"),
        "AttemptId" => Some("e"),
        "MessageId" => Some("m"),
        "WorkAreaId" => Some("a"),
        "CandidateId" => Some("c"),
        "CoordinationId" => Some("n"),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn request_ids_are_optional_but_explicit_retry_ids_are_preserved() {
        let (first, _) = decode("context", json!({})).unwrap();
        let (second, _) = decode("context", json!({})).unwrap();
        assert_ne!(first, second);
        let (retry, _) = decode("context", json!({"request_id":first})).unwrap();
        assert_eq!(retry, first);
        let context = catalog()
            .iter()
            .find(|tool| tool["name"] == "context")
            .unwrap();
        assert!(
            !context["inputSchema"]["required"]
                .as_array()
                .unwrap()
                .contains(&json!("request_id"))
        );
    }

    #[test]
    fn the_models_tool_is_a_read_without_arguments() {
        let tool = catalog()
            .iter()
            .find(|tool| tool["name"] == "models")
            .unwrap();
        assert_eq!(tool["annotations"]["readOnlyHint"], true);
        let (_, command) = decode("models", json!({})).unwrap();
        assert!(matches!(command, Command::Models));
    }

    #[test]
    fn candidate_revision_is_a_sibling_of_its_flattened_plan() {
        let tool = catalog()
            .iter()
            .find(|tool| tool["name"] == "propose_candidate")
            .unwrap();
        assert!(
            tool["inputSchema"]["properties"]
                .get("expected_revision")
                .is_some()
        );
        let arguments = json!({"id":adroit_model::CandidateId::new(),"run_id":adroit_model::RunId::new(),"source":{"task_id":adroit_model::TaskId::new(),"revision":1},"attempt_id":adroit_model::AttemptId::new(),"expected_revision":7,"inputs":[]});
        let (_, command) = decode("propose_candidate", arguments).unwrap();
        assert!(matches!(
            command,
            Command::ProposeCandidate {
                expected_revision: 7,
                ..
            }
        ));
    }
}
