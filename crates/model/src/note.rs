//! A message a session sent or received through Adroit.

use serde::{Deserialize, Serialize};

use crate::SessionCode;
use crate::time::ago;

/// A session that may ask another for work, or has asked it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Requester {
    pub code: SessionCode,
    pub name: String,
    /// Whether what it asks is taken up as work.
    pub taken_up: bool,
}

/// What became of the work a message asked for.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RequestOutcome {
    Waiting,
    TakenUp,
    /// Told as information.
    Told,
}

/// A message the session shown sent or received through Adroit, as the
/// overview lists it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Note {
    pub id: String,
    /// Whether the session shown sent it.
    pub outgoing: bool,
    /// The session on the other end, by name.
    pub other: String,
    pub body: String,
    /// What became of the work it asked for; none for a message.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub asked: Option<RequestOutcome>,
    /// Whether it is the user's own words, which they approved to be passed
    /// on.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub relayed: bool,
    pub sent_ms: u64,
}

impl Note {
    /// Its first line, to list it by.
    pub fn summary(&self) -> &str {
        self.body.lines().next().unwrap_or_default()
    }

    /// Who it went between, `now_ms` telling how long ago.
    pub fn tag(&self, now_ms: u64) -> String {
        let what = match self.asked {
            _ if self.relayed => "your words ",
            None => "",
            Some(RequestOutcome::Waiting) => "request ",
            Some(RequestOutcome::TakenUp) => "work ",
            Some(RequestOutcome::Told) => "information ",
        };
        let way = match (self.outgoing, self.relayed) {
            (true, _) => "to",
            (false, false) => "from",
            (false, true) => "through",
        };
        format!(
            "{what}{way} {} · {}",
            self.other,
            ago(now_ms.saturating_sub(self.sent_ms))
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_message_s_tag_says_what_became_of_the_work_it_asked_for() {
        let tag = |asked| {
            Note {
                id: "n".into(),
                outgoing: false,
                other: "lead".into(),
                body: String::new(),
                asked,
                relayed: false,
                sent_ms: 0,
            }
            .tag(0)
        };
        for (asked, begins) in [
            (None, "from lead"),
            (Some(RequestOutcome::Waiting), "request from lead"),
            (Some(RequestOutcome::TakenUp), "work from lead"),
            (Some(RequestOutcome::Told), "information from lead"),
        ] {
            assert!(tag(asked).starts_with(begins), "{}", tag(asked));
        }
    }

    #[test]
    fn the_user_s_words_are_tagged_as_theirs_with_the_session_they_went_through() {
        let tag = |outgoing| {
            Note {
                id: "n".into(),
                outgoing,
                other: "lead".into(),
                body: String::new(),
                asked: None,
                relayed: true,
                sent_ms: 0,
            }
            .tag(0)
        };
        assert!(tag(false).starts_with("your words through lead"));
        assert!(tag(true).starts_with("your words to lead"));
    }
}
