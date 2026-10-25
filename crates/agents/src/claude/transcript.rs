//! A Claude Code transcript read as it grows, with its subagents' and its
//! conversation's numbers.

use std::path::{Path, PathBuf};

use super::subagents::Subagents;
use crate::links::{Collector, Links};
use crate::tail::{Tail, Tailed};

/// A transcript read as it grows: each read takes in only what was written
/// since the last.
#[derive(Debug, Clone)]
pub struct Transcript {
    path: PathBuf,
    tail: Tail,
    found: Collector,
    agents: Subagents,
    metrics: super::metrics::Collector,
}

impl Transcript {
    /// The transcript at `path` of a session working in `cwd`.
    pub fn new(
        path: impl Into<PathBuf>,
        cwd: impl Into<PathBuf>,
        home: impl Into<PathBuf>,
    ) -> Self {
        let path = path.into();
        Transcript {
            agents: Subagents::new(&path),
            metrics: super::metrics::Collector::default(),
            path,
            tail: Tail::default(),
            found: Collector::new(cwd, home),
        }
    }

    /// Take in what was written since the last read; whether what it brings
    /// up may have changed. One that shrank, was replaced or is gone is read
    /// from its start. The transcripts of its subagents are read too.
    pub fn read(&mut self, exists: &dyn Fn(&Path) -> bool) -> bool {
        let changed = self.read_own(exists);
        self.agents.look() || changed
    }

    fn read_own(&mut self, exists: &dyn Fn(&Path) -> bool) -> bool {
        let had = self.tail.opened();
        let Tailed::Lines {
            restarted,
            ended,
            values,
        } = self.tail.read(&self.path)
        else {
            self.tail = Tail::default();
            self.restart();
            return had;
        };
        if restarted {
            self.restart();
        }
        for v in values {
            self.found.take(&v, exists);
            self.agents.feed(&v);
            self.metrics.feed(&v);
        }
        restarted || ended
    }

    pub fn links(&self) -> Links {
        Links {
            agents: self.agents.agents(),
            ..self.found.links()
        }
    }

    pub fn stats(&self) -> adroit_model::ConversationStats {
        self.metrics.stats()
    }

    fn restart(&mut self) {
        self.metrics = super::metrics::Collector::default();
        self.agents = Subagents::new(&self.path);
        self.found.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use crate::links::tests::{said, targets, tool};
    use crate::testing::append;
    use serde_json::json;

    fn scratch(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("adroit-links-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir.join("talk.jsonl")
    }

    const NOWHERE: &dyn Fn(&Path) -> bool = &|_| false;

    #[test]
    fn conversation_metrics_follow_complete_appends_and_reset_with_the_transcript() {
        let path = scratch("metrics");
        let first = json!({"type":"assistant","message":{"id":"m1","usage":{"input_tokens":10,"output_tokens":2}}}).to_string();
        std::fs::write(&path, format!("{first}\n")).unwrap();
        let mut transcript = Transcript::new(&path, "/w", "/home/me");
        transcript.read(NOWHERE);
        assert_eq!(transcript.stats().tokens.unwrap().total(), 12);
        assert!(!transcript.read(NOWHERE));
        assert_eq!(transcript.stats().tokens.unwrap().total(), 12);
        let next = json!({"type":"assistant","message":{"id":"m2","usage":{"input_tokens":20,"output_tokens":3}}}).to_string();
        append(&path, &next);
        transcript.read(NOWHERE);
        assert_eq!(transcript.stats().tokens.unwrap().total(), 12);
        append(&path, "\n");
        transcript.read(NOWHERE);
        assert_eq!(transcript.stats().tokens.unwrap().total(), 35);
        std::fs::write(&path, format!("{next}\n")).unwrap();
        transcript.read(NOWHERE);
        assert_eq!(transcript.stats().tokens.unwrap().total(), 23);
        std::fs::remove_file(&path).unwrap();
        transcript.read(NOWHERE);
        assert_eq!(
            transcript.stats(),
            adroit_model::ConversationStats::default()
        );
    }

    #[test]
    fn a_transcript_is_read_on_from_where_it_was_left_a_partial_line_awaiting_its_end() {
        let path = scratch("appended");
        append(&path, &format!("{}\n", said("https://a.dev")));
        let mut t = Transcript::new(&path, "/w", "/home/me");
        assert!(t.read(NOWHERE));
        assert!(!t.read(NOWHERE), "nothing new");
        let line = said("https://b.dev");
        let (first, second) = line.split_at(10);
        append(&path, first);
        assert!(!t.read(NOWHERE), "half a line");
        append(&path, &format!("{second}\n"));
        assert!(t.read(NOWHERE));
        assert_eq!(targets(&t.links().urls), ["https://b.dev", "https://a.dev"]);
    }

    #[test]
    fn a_transcript_that_shrank_or_was_replaced_is_read_again_from_its_start() {
        let path = scratch("replaced");
        append(
            &path,
            &format!("{}\n{}\n", said("https://a.dev"), said("https://b.dev")),
        );
        let mut t = Transcript::new(&path, "/w", "/home/me");
        assert!(t.read(NOWHERE));
        std::fs::write(&path, format!("{}\n", said("https://c.dev"))).unwrap();
        assert!(t.read(NOWHERE));
        assert_eq!(targets(&t.links().urls), ["https://c.dev"]);
        std::fs::remove_file(&path).unwrap();
        assert!(t.read(NOWHERE));
        assert!(t.links().is_empty());
    }

    #[test]
    fn a_transcript_lists_the_subagents_it_runs_with_their_steps() {
        use crate::claude::subagents::{Standing, Step};
        let path = scratch("agents");
        let own = path.with_extension("").join("subagents");
        std::fs::create_dir_all(&own).unwrap();
        append(
            &path,
            &format!(
                "{}\n{}\n",
                tool(
                    "toolu_a",
                    "Agent",
                    json!({"description": "Survey", "subagent_type": "Explore", "prompt": "Look."})
                ),
                json!({
                    "type": "user",
                    "message": {"role": "user", "content": [
                        {"type": "tool_result", "tool_use_id": "toolu_a", "content": "launched"}
                    ]},
                    "toolUseResult": {"isAsync": true, "status": "async_launched", "agentId": "a7cd"},
                }),
            ),
        );
        let mut t = Transcript::new(&path, "/w", "/home/me");
        assert!(t.read(NOWHERE));
        assert_eq!(t.links().agents[0].standing, Standing::Running);
        assert!(t.links().agents[0].steps.is_empty());
        assert!(!t.links().is_empty());
        append(
            &own.join("agent-a7cd.jsonl"),
            &format!("{}\n", said("Reading the code.")),
        );
        assert!(t.read(NOWHERE), "its own transcript grew");
        assert_eq!(
            t.links().agents[0].steps,
            [Step::Said("Reading the code.".into())]
        );
    }
}
