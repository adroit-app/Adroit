//! The sessions and terminals a terminal host took with it, waiting to be
//! brought back.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::profile::ProfileId;
use crate::{Agent, TerminalProgram};

/// A session or a terminal that ended with the terminal host it ran in.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Interrupted {
    /// The daemon's id it ran as, which its tabs still name.
    pub id: String,
    pub name: String,
    pub cwd: PathBuf,
    /// When it started, in milliseconds since the Unix epoch.
    pub started_ms: u64,
    pub what: InterruptedKind,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum InterruptedKind {
    /// An agent's session, brought back by resuming `conversation`; none
    /// when the agent never told it.
    Session {
        agent: Agent,
        profile: ProfileId,
        conversation: Option<String>,
    },
    /// A shell or a Neovim, opened again in its directory.
    Terminal {
        program: TerminalProgram,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        file: Option<PathBuf>,
    },
}

impl Interrupted {
    pub fn is_session(&self) -> bool {
        matches!(self.what, InterruptedKind::Session { .. })
    }
}

/// What came of bringing back one interrupted session or terminal.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BroughtBack {
    /// The daemon's id it ran as.
    pub id: String,
    pub name: String,
    /// The daemon's id of the session or terminal that stands for it now.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub by: Option<String>,
    /// Why nothing stands for it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub refused: Option<String>,
}
