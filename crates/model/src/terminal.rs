use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::TerminalResourceId;

/// A terminal program, independent of any agent conversation or task result.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct TerminalSession {
    pub resource_id: TerminalResourceId,
    /// The daemon's runtime ID, separate from the stable resource identity.
    #[serde(skip)]
    pub id: u128,
    pub name: String,
    pub cwd: PathBuf,
    pub program: TerminalProgram,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TerminalProgram {
    #[default]
    Shell,
    Neovim,
}
