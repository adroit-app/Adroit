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
    /// Whether it is sent so that a reply lost is found again, for which a
    /// client keeps it and its answer in a journal. Giving a key is not: its
    /// answer is the key, and one whose answer was lost is taken back from
    /// the list of them.
    pub fn mutates(&self) -> bool {
        !matches!(
            self,
            Self::TerminalHost
                | Self::PathEntries { .. }
                | Self::Drafts
                | Self::OutsideKeys
                | Self::IssueOutsideKey { .. }
                | Self::Picture { .. }
                | Self::LoginStanding { .. }
                | Self::Cloned { .. }
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
            | Self::HandBackName { .. }
            | Self::TakeRequests { .. }
            | Self::ApproveRelay { .. }
            | Self::DeclineRelay { .. }
            | Self::Release { .. }
            | Self::EndTerminal { .. }
            | Self::KeepWorkspace { .. }
            | Self::DropWorkspace { .. } => true,
            _ => false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_key_given_is_kept_in_no_journal_and_one_taken_back_is() {
        let given = Ask::IssueOutsideKey { name: "a".into() };
        assert!(!given.mutates());
        assert!(!Ask::OutsideKeys.mutates());
        let id = adroit_model::MessageId::new();
        assert!(Ask::RevokeOutsideKey { id }.mutates());
    }
}
