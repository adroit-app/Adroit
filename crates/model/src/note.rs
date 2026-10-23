//! A message a session sent or received through Adroit.

use serde::{Deserialize, Serialize};

use crate::time::ago;

/// A message the session shown sent or received through Adroit, as the
/// overview lists it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Note {
    pub id: String,
    /// Whether the session shown sent it.
    pub outgoing: bool,
    /// The session on the other end, by name; Adroit for its own notices.
    pub other: String,
    /// `message`, `report` or `notice`.
    pub kind: String,
    pub body: String,
    pub sent_ms: u64,
}

impl Note {
    /// Its first line, to list it by.
    pub fn summary(&self) -> &str {
        self.body.lines().next().unwrap_or_default()
    }

    /// Who it went between and what it is, `now_ms` telling how long ago.
    pub fn tag(&self, now_ms: u64) -> String {
        let way = if self.outgoing { "to" } else { "from" };
        format!(
            "{} · {way} {} · {}",
            self.kind,
            self.other,
            ago(now_ms.saturating_sub(self.sent_ms))
        )
    }
}
