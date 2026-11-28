//! Admission and results of changes whose replies may be lost.

use crate::{Answer, Ask, Request};
use serde::{Deserialize, Serialize};

/// What a work host answers a paired client that asks for a change without
/// an ID. A host of an earlier build answers it also to a command that only
/// reads, which it took for a change.
pub const CHANGE_NEEDS_AN_ID: &str = "remote changes require an idempotent mutation request";

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
        if let Self::Run { args, .. } = self {
            return !reads(args);
        }
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

/// Whether `adroit <args>` only reads, so that one whose answer was lost is
/// asked again rather than found in a journal: a watch asks every while, and
/// would fill the journal with reads.
fn reads(args: &[String]) -> bool {
    match args {
        [on, operation, ..] if on == "session" => matches!(
            operation.as_str(),
            "list" | "show" | "changes" | "profiles" | "wait" | "watch" | "explain"
        ),
        [on, operation, ..] if on == "terminal" => {
            matches!(operation.as_str(), "list" | "screen" | "wait")
        }
        _ => false,
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

    fn run(args: &[&str]) -> Ask {
        Ask::Run {
            args: args.iter().map(|arg| arg.to_string()).collect(),
            cwd: "/".into(),
            input: None,
        }
    }

    #[test]
    fn a_command_that_only_reads_is_no_change_and_one_that_acts_is() {
        for reads in [
            &["session", "list", "--json"][..],
            &["session", "show", "r4pj"],
            &["session", "changes", "r4pj"],
            &["session", "profiles"],
            &["session", "wait", "r4pj"],
            &["session", "watch", "--once", "--since", "3"],
            &["session", "explain", "r4pj", "--json"],
            &["terminal", "list"],
            &["terminal", "screen", "--terminal", "t"],
            &["terminal", "wait", "--terminal", "t"],
        ] {
            assert!(!run(reads).mutates(), "{reads:?}");
        }
        for acts in [
            &["session", "start", "--dir", "/w"][..],
            &["session", "send", "r4pj", "--from", "a"],
            &["session", "answer", "r4pj"],
            &["session", "interrupt", "r4pj"],
            &["session", "end", "r4pj"],
            &["session", "resume", "r4pj"],
            &["session", "open", "r4pj"],
            &["terminal", "start"],
            &["terminal", "input", "--terminal", "t"],
            &["terminal", "stop", "--terminal", "t"],
            &["session"],
            &["list"],
        ] {
            assert!(run(acts).mutates(), "{acts:?}");
        }
    }
}
