//! The subagents a session runs: how each stands and the steps it took.

/// How a subagent stands.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Standing {
    Running,
    /// It finished its work.
    Done,
    Failed,
    /// It was stopped, or its session ended before it finished.
    Stopped,
}

/// A step a subagent took.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Step {
    /// A tool it called, and what the call was about.
    Tool { name: String, about: String },
    /// What it said, or handed back as its report.
    Said(String),
}

/// A subagent a session started through Claude Code's `Agent` tool.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Agent {
    /// The id of the tool call that started it, which tells it apart.
    pub call: String,
    /// The id Claude Code gave it, which names its own transcript.
    pub id: Option<String>,
    pub description: String,
    /// What it is about now: the summary of the latest message the session
    /// delivered to it, as Claude Code's own list of agents names it.
    pub title: Option<String>,
    /// The kind of agent asked for, as `Explore`.
    pub kind: Option<String>,
    pub prompt: String,
    pub standing: Standing,
    /// When it was started, in milliseconds since the Unix epoch.
    pub started_ms: Option<u64>,
    /// When it last ended, in milliseconds since the Unix epoch.
    pub ended_ms: Option<u64>,
    /// Its latest steps, the last last.
    pub steps: Vec<Step>,
}
