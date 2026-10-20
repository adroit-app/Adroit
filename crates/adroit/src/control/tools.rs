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
        let required = schema["required"].as_array_mut().unwrap();
        required.retain(|field| field != "op");
        required.push(json!("request_id"));
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
        if !reachable.is_empty() { schema["$defs"] = Value::Object(reachable); }
        schema.as_object_mut().unwrap().remove("title");
        let mut tool = json!({
            "name":name,
            "description":format!("Adroit {name}: {} Allocate request_id with new_ids before the first call. Pass the declared fields directly; do not add command or op wrappers. Keep the same request_id only when retrying the same immutable operation.", description(name)),
            "inputSchema":schema
        });
        if matches!(name, "context" | "snapshot" | "read_message" | "mailbox" | "events" | "candidate") {
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
        .ok_or("request_id is required; allocate it with new_ids")?;
    let request = serde_json::from_value(request)
        .map_err(|_| "request_id must be a UUID allocated with new_ids")?;
    let mut command = if let Some(field) = wrapper(name) {
        json!({(field):arguments})
    } else {
        Value::Object(arguments)
    };
    command["op"] = json!(name);
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
            "Acknowledge one addressed immutable message ID before handling it; do not repeat already handled effects."
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
