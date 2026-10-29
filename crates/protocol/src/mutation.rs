//! Admission and results of changes whose replies may be lost.

use crate::{Answer, Ask, Request};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MutationStamp {
    pub instance: String,
    pub work: String,
    pub terminal: String,
    pub epoch: u64,
    pub id: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "state", rename_all = "snake_case")]
pub enum MutationState {
    Accepted,
    Result { answer: Box<Answer> },
    Unknown,
    Retired,
    Conflict,
    Full,
    Rejected { text: String },
}

impl Ask {
    pub fn mutates(&self) -> bool {
        !matches!(
            self,
            Self::TerminalHost
                | Self::PathEntries { .. }
                | Self::Drafts
                | Self::Picture { .. }
                | Self::LoginStanding { .. }
                | Self::Apply { .. }
                | Self::MutationStatus { .. }
                | Self::MutationEpoch
        )
    }
}

impl Request {
    pub fn mutates(&self) -> bool {
        match self {
            Self::Ask { ask, .. } => ask.mutates(),
            Self::Stop { .. }
            | Self::Kill { .. }
            | Self::Rename { .. }
            | Self::Release { .. }
            | Self::EndTerminal { .. } => true,
            _ => false,
        }
    }
}
