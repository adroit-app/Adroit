use serde::{Deserialize, Serialize};

/// A command `/` offers.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Command {
    /// Without the leading `/`; a Codex skill keeps its `$` invocation prefix.
    pub name: String,
    pub description: String,
    /// What follows the name, as Claude Code hints it; empty when nothing does.
    pub argument_hint: String,
}
