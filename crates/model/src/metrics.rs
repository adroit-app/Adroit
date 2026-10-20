//! Provider-reported facts about one conversation.

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Tokens {
    /// All input tokens, including cache reads and writes.
    pub input: u64,
    pub output: u64,
    /// Cache reads, already included in input.
    pub cached: u64,
}

impl Tokens {
    pub fn total(self) -> u64 {
        self.input.saturating_add(self.output)
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ConversationStats {
    pub tokens: Option<Tokens>,
    pub model: Option<String>,
    pub last_user_ms: Option<u64>,
    pub user_turns: u64,
    pub tool_calls: u64,
    /// Current context tokens and the reported context window.
    pub context: Option<(u64, u64)>,
}
