//! The user's words a session proposes to pass on to other sessions.

use serde::{Deserialize, Serialize};

use crate::{MessageId, SessionCode};

/// A session words are to reach.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RelayTo {
    pub code: SessionCode,
    pub name: String,
}

/// Words a session proposes to pass on as the user's own, which go nowhere
/// until the user approves them.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Relay {
    pub id: MessageId,
    /// The session that proposes them, by its code and its name.
    pub from: SessionCode,
    pub from_name: String,
    /// The sessions they are to reach that still run.
    pub to: Vec<RelayTo>,
    pub body: String,
    pub proposed_ms: u64,
}
