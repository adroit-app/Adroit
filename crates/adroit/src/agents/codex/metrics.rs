//! Metrics from Codex rollout records.

use std::collections::HashSet;

use adroit_claude::metrics::timestamp_ms;
use adroit_model::{ConversationStats, Tokens};
use serde_json::Value;

#[derive(Default)]
pub(super) struct Collector {
    stats: ConversationStats,
    users: HashSet<String>,
    user_events: HashSet<String>,
    tools: HashSet<String>,
}

impl Collector {
    pub(super) fn feed(&mut self, record: &Value) {
        let payload = &record["payload"];
        match record["type"].as_str() {
            Some("turn_context") => {
                if let Some(model) = payload["model"].as_str().filter(|s| !s.is_empty()) {
                    self.stats.model = Some(model.into());
                }
            }
            Some("event_msg") => match payload["type"].as_str() {
                Some("token_count") => {
                    let info = &payload["info"];
                    let usage = &info["total_token_usage"];
                    if usage.is_object() {
                        self.stats.tokens = Some(Tokens {
                            input: usage["input_tokens"].as_u64().unwrap_or(0),
                            output: usage["output_tokens"].as_u64().unwrap_or(0),
                            cached: usage["cached_input_tokens"].as_u64().unwrap_or(0),
                        });
                    }
                    if let (Some(used), Some(total)) = (
                        info["last_token_usage"]["total_tokens"].as_u64(),
                        info["model_context_window"].as_u64().filter(|n| *n > 0),
                    ) {
                        self.stats.context = Some((used, total));
                    }
                }
                Some("user_message") => {
                    if let Some(id) = payload["turn_id"]
                        .as_str()
                        .or_else(|| record["timestamp"].as_str())
                    {
                        self.user_events.insert(id.into());
                    }
                    self.stats.last_user_ms = self
                        .stats
                        .last_user_ms
                        .max(timestamp_ms(&record["timestamp"]));
                }
                _ => {}
            },
            Some("response_item") => match payload["type"].as_str() {
                Some("message") if payload["role"] == "user" && human_message(payload) => {
                    let meta = &payload["internal_chat_message_metadata_passthrough"];
                    if let Some(id) = meta["turn_id"]
                        .as_str()
                        .or_else(|| payload["id"].as_str())
                        .or_else(|| record["timestamp"].as_str())
                    {
                        self.users.insert(id.into());
                    }
                    self.stats.last_user_ms = self
                        .stats
                        .last_user_ms
                        .max(timestamp_ms(&record["timestamp"]));
                }
                Some(
                    "function_call" | "custom_tool_call" | "web_search_call" | "local_shell_call",
                ) => {
                    if let Some(id) = payload["call_id"]
                        .as_str()
                        .or_else(|| payload["id"].as_str())
                    {
                        self.tools.insert(id.into());
                    }
                }
                _ => {}
            },
            _ => {}
        }
    }

    pub(super) fn stats(&self) -> ConversationStats {
        let mut stats = self.stats.clone();
        stats.user_turns = if self.user_events.is_empty() {
            self.users.len()
        } else {
            self.user_events.len()
        } as u64;
        stats.tool_calls = self.tools.len() as u64;
        stats
    }
}

fn human_message(payload: &Value) -> bool {
    if let Some(kinds) =
        payload["internal_chat_message_metadata_passthrough"]["content_item_kinds"].as_array()
    {
        return kinds
            .iter()
            .filter_map(Value::as_str)
            .any(|kind| kind.starts_with("user."));
    }
    payload["content"].as_array().is_some_and(|parts| {
        parts.iter().any(|part| {
            if matches!(part["type"].as_str(), Some("input_image" | "image")) {
                return true;
            }
            let Some(text) = part["text"].as_str() else {
                return false;
            };
            let text = text.trim();
            !text.is_empty()
                && ![
                    "# AGENTS.md instructions",
                    "<environment_context>",
                    "<INSTRUCTIONS>",
                    "<permissions instructions>",
                    "<turn_aborted>",
                    "<subagent_notification>",
                ]
                .iter()
                .any(|prefix| text.starts_with(prefix))
        })
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn cumulative_usage_replaces_totals_and_cached_input_is_not_added_twice() {
        let mut collector = Collector::default();
        collector.feed(&json!({"type":"turn_context","payload":{"model":"codex-test"}}));
        let record = json!({"type":"event_msg","payload":{"type":"token_count","info":{
            "total_token_usage":{"input_tokens":100,"cached_input_tokens":40,"output_tokens":20,"total_tokens":120},
            "last_token_usage":{"total_tokens":60},"model_context_window":200}}});
        collector.feed(&record);
        collector.feed(&record);
        let stats = collector.stats();
        let tokens = stats.tokens.unwrap();
        assert_eq!((tokens.input, tokens.output, tokens.cached), (100, 20, 40));
        assert_eq!(stats.model.as_deref(), Some("codex-test"));
        assert_eq!(stats.context, Some((60, 200)));
        collector.feed(&json!({"type":"event_msg","payload":{"type":"token_count","info":null}}));
        assert_eq!(collector.stats(), stats);
    }

    #[test]
    fn only_human_content_counts_as_a_turn_and_duplicate_calls_are_counted_once() {
        let mut collector = Collector::default();
        for kind in [
            "agents_md.instructions",
            "environments.environment_context",
            "user.text",
        ] {
            let record = json!({"type":"response_item","timestamp":"2026-10-20T00:00:00Z","payload":{
                "type":"message","id":kind,"role":"user","content":[{"type":"input_text","text":"input"}],
                "internal_chat_message_metadata_passthrough":{"turn_id":"turn1","content_item_kinds":[kind]}}});
            collector.feed(&record);
            collector.feed(&record);
        }
        for _ in 0..2 {
            collector.feed(&json!({"type":"response_item","payload":{"type":"function_call","call_id":"tool1"}}));
        }
        let stats = collector.stats();
        assert_eq!(stats.user_turns, 1);
        assert_eq!(stats.tool_calls, 1);
        assert_eq!(stats.last_user_ms, Some(1792454400000));
    }
}
