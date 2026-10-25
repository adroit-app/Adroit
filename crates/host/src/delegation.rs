//! Sessions that open sessions beside them and message one another.
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
    /// The session that opened this one; none for a session the user started.
    pub parent: Option<SessionId>,
    pub agent: Agent,
    /// The profile it started from, and the folder of that profile's own it
    /// ran in, none for its agent's own environment.
    pub profile: adroit_model::profile::ProfileId,
    pub home: Option<PathBuf>,
    pub name: String,
    pub cwd: PathBuf,
    pub terminal: Option<u128>,
    pub conversation: Option<String>,
    pub readiness: Readiness,
    pub started_ms: u64,
    pub ended: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Message {
    pub id: MessageId,
    pub sender: SessionId,
    pub recipient: SessionId,
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
