//! Conversation metrics reported by Claude Code transcripts.

use std::collections::{HashMap, HashSet};

use adroit_model::{ConversationStats, Tokens};
use serde_json::Value;

use crate::time::timestamp_ms;

#[derive(Debug, Clone, Default)]
pub struct Collector {
    stats: ConversationStats,
    messages: HashMap<String, Tokens>,
    users: HashSet<String>,
    tools: HashSet<String>,
}

impl Collector {
    pub fn feed(&mut self, record: &Value) {
        if record["isSidechain"].as_bool() == Some(true) {
            return;
        }
        let message = &record["message"];
        match record["type"].as_str() {
            Some("assistant") => {
                if let Some(model) = message["model"]
                    .as_str()
                    .filter(|s| !s.is_empty() && !s.starts_with('<'))
                {
                    self.stats.model = Some(model.into());
                }
                let usage = &message["usage"];
                if usage.is_object()
                    && let Some(id) = message["id"].as_str().or_else(|| record["uuid"].as_str())
                {
                    let n = |key: &str| usage[key].as_u64().unwrap_or(0);
                    let next = Tokens {
                        input: n("input_tokens")
                            .saturating_add(n("cache_read_input_tokens"))
                            .saturating_add(n("cache_creation_input_tokens")),
                        output: n("output_tokens"),
                        cached: n("cache_read_input_tokens"),
                    };
                    let held = self.messages.entry(id.into()).or_default();
                    held.input = held.input.max(next.input);
                    held.output = held.output.max(next.output);
                    held.cached = held.cached.max(next.cached);
                }
                if let Some(parts) = message["content"].as_array() {
                    for part in parts {
                        if part["type"] == "tool_use"
                            && let Some(id) = part["id"].as_str()
                        {
                            self.tools.insert(id.into());
                        }
                    }
                }
            }
            Some("user") => {
                if record["isMeta"].as_bool() == Some(true)
                    || record["isCompactSummary"].as_bool() == Some(true)
                    || matches!(
                        record["origin"]["kind"].as_str(),
                        Some("peer" | "agent" | "system")
                    )
                {
                    return;
                }
                let content = &message["content"];
                let human = content.as_str().is_some_and(|s| !s.trim().is_empty())
                    || content.as_array().is_some_and(|parts| {
                        parts
                            .iter()
                            .any(|p| matches!(p["type"].as_str(), Some("text" | "image")))
                    });
                if human {
                    if let Some(id) = record["uuid"]
                        .as_str()
                        .or_else(|| record["timestamp"].as_str())
                    {
                        self.users.insert(id.into());
                    }
                    self.stats.last_user_ms = self
                        .stats
                        .last_user_ms
                        .max(timestamp_ms(&record["timestamp"]));
                }
            }
            _ => {}
        }
    }

    pub fn stats(&self) -> ConversationStats {
        let mut stats = self.stats.clone();
        if !self.messages.is_empty() {
            stats.tokens = Some(
                self.messages
                    .values()
                    .fold(Tokens::default(), |mut total, n| {
                        total.input = total.input.saturating_add(n.input);
                        total.output = total.output.saturating_add(n.output);
                        total.cached = total.cached.saturating_add(n.cached);
                        total
                    }),
            );
        }
        stats.user_turns = self.users.len() as u64;
        stats.tool_calls = self.tools.len() as u64;
        stats
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn assistant_usage_includes_cached_input_and_replaces_streamed_snapshots() {
        let mut collector = Collector::default();
        let mut message = json!({"type":"assistant","message":{"id":"m1","model":"claude-test",
            "usage":{"input_tokens":10,"cache_read_input_tokens":20,"cache_creation_input_tokens":30,"output_tokens":4},
            "content":[{"type":"tool_use","id":"t1","name":"Read"}]}});
        collector.feed(&message);
        collector.feed(&message);
        message["message"]["usage"]["output_tokens"] = json!(8);
        collector.feed(&message);
        collector.feed(&json!({"type":"assistant","message":{"id":"m2","usage":{"input_tokens":5,"output_tokens":2}}}));
        let stats = collector.stats();
        let tokens = stats.tokens.unwrap();
        assert_eq!((tokens.input, tokens.output, tokens.cached), (65, 10, 20));
        assert_eq!(stats.model.as_deref(), Some("claude-test"));
        assert_eq!(stats.tool_calls, 1);
    }

    #[test]
    fn user_turns_exclude_tool_results_metadata_sidechains_and_peer_messages() {
        let mut collector = Collector::default();
        let input = json!({"type":"user","uuid":"u1","timestamp":"2026-10-20T00:00:00Z","message":{"content":"hello"}});
        collector.feed(&input);
        collector.feed(&input);
        for extra in [
            json!({"type":"user","isMeta":true,"message":{"content":"instructions"}}),
            json!({"type":"user","isSidechain":true,"message":{"content":"child"}}),
            json!({"type":"user","origin":{"kind":"peer"},"message":{"content":"message"}}),
            json!({"type":"user","message":{"content":[{"type":"tool_result","content":"done"}]}}),
        ] {
            collector.feed(&extra);
        }
        let stats = collector.stats();
        assert_eq!(stats.user_turns, 1);
        assert_eq!(stats.last_user_ms, Some(1792454400000));
        assert!(stats.tokens.is_none());
    }
}
