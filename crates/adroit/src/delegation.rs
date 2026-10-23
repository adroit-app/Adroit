//! Sessions that start sessions, message one another and report back.
use std::{fmt, path::PathBuf};

use adroit_model::Agent;
pub use adroit_model::{MessageId, SessionId};
use serde::{Deserialize, Serialize};

pub mod app;
pub mod bridge;
pub mod launch;
pub mod protocol;
pub mod service;
pub mod store;
pub mod worktree;

/// How a session stands while its agent starts.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Readiness {
    Starting,
    Ready,
    /// A dialog waits for the user in the session's console.
    Waiting,
    /// Neither ready nor showing a dialog long after it started.
    Stalled,
}

impl Readiness {
    const ALL: [Self; 4] = [Self::Starting, Self::Ready, Self::Waiting, Self::Stalled];

    pub fn key(self) -> &'static str {
        match self {
            Self::Starting => "starting",
            Self::Ready => "ready",
            Self::Waiting => "waiting",
            Self::Stalled => "stalled",
        }
    }

    fn from_key(key: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|known| known.key() == key)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Session {
    pub id: SessionId,
    /// The session that started this one; none for a session the user started.
    pub parent: Option<SessionId>,
    pub agent: Agent,
    pub name: String,
    pub cwd: PathBuf,
    pub terminal: Option<u128>,
    pub conversation: Option<String>,
    pub readiness: Readiness,
    pub started_ms: u64,
    pub reported: bool,
    pub ended: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MessageKind {
    Message,
    Report,
    /// Adroit's own word about a session, which no session sent.
    Notice,
}

impl MessageKind {
    const ALL: [Self; 3] = [Self::Message, Self::Report, Self::Notice];

    pub fn key(self) -> &'static str {
        match self {
            Self::Message => "message",
            Self::Report => "report",
            Self::Notice => "notice",
        }
    }

    fn from_key(key: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|known| known.key() == key)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Message {
    pub id: MessageId,
    pub sender: Option<SessionId>,
    pub recipient: SessionId,
    pub kind: MessageKind,
    pub body: String,
    pub sent_ms: u64,
    pub delivered: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Code {
    Invalid,
    NotFound,
    Refused,
    /// A newer adroit has written the state.
    Unsupported,
    Storage,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Error {
    pub code: Code,
    pub message: String,
}

impl Error {
    pub fn new(code: Code, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
        }
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.message)
    }
}

impl std::error::Error for Error {}

pub type Result<T> = std::result::Result<T, Error>;

pub(crate) use adroit_model::time::now_ms;
