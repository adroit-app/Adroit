use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::{SessionId, TerminalResourceId};

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
    /// The file Neovim was started on, if it was.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub file: Option<PathBuf>,
    /// The program and its arguments a command terminal runs.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub command: Vec<String>,
    /// How its program last reported it stands; none while it reported nothing.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub state: Option<crate::State>,
    /// The session that started it, by Adroit's record of that session.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub opener: Option<SessionId>,
    /// Whether the session that started it asked that it be shown to the user.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub show: bool,
    /// The exit status of a command terminal's program once it has ended; the
    /// terminal keeps its screen until it is stopped.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub exit: Option<i32>,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TerminalProgram {
    #[default]
    Shell,
    Neovim,
    /// Any program, started through the shell.
    Command,
}

/// The name a terminal is given unless one is asked for.
pub fn base_name(program: TerminalProgram, command: &[String]) -> String {
    match program {
        TerminalProgram::Shell => "shell".into(),
        TerminalProgram::Neovim => "nvim".into(),
        TerminalProgram::Command => command
            .first()
            .and_then(|p| std::path::Path::new(p).file_name())
            .map_or_else(|| "command".into(), |n| n.to_string_lossy().into_owned()),
    }
}
