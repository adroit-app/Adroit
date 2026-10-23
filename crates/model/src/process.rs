//! A process running for a session, as the system lists it.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

/// A live process of a session's, or one under it.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Process {
    pub key: String,
    pub pid: u32,
    pub parent: Option<u32>,
    pub name: String,
    pub command: String,
    pub cwd: Option<PathBuf>,
    pub memory: u64,
    pub started_secs: u64,
    pub zombie: bool,
    /// The account it runs as.
    pub owner: Option<String>,
    /// The program file it runs.
    pub executable: Option<PathBuf>,
}
