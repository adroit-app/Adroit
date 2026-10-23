//! What the prompt asks of the work host: the prompts sent before in a
//! project, read afresh each time the user steps back through them.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

/// The prompts an agent kept as sent in a project, oldest first, read for
/// the request numbered `asked`.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct HistoryRead {
    pub asked: u64,
    pub project: PathBuf,
    pub sent: Vec<String>,
}

/// What the host read for the prompt.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct PromptWork {
    /// The history last read; none until one is asked for.
    pub history: Option<HistoryRead>,
}

/// One change to what the host read for the prompt.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum PromptChange {
    History { read: HistoryRead },
}

impl PromptWork {
    /// What was read with `change` made; whether that changed it.
    pub fn apply(&mut self, change: PromptChange) -> bool {
        match change {
            PromptChange::History { read } => {
                if self.history.as_ref() == Some(&read) {
                    return false;
                }
                self.history = Some(read);
                true
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn each_request_s_history_replaces_the_one_before() {
        let mut work = PromptWork::default();
        let read = |asked| PromptChange::History {
            read: HistoryRead {
                asked,
                project: "/w".into(),
                sent: vec!["fix it".into()],
            },
        };
        assert!(work.apply(read(1)));
        assert!(!work.apply(read(1)));
        assert!(work.apply(read(2)), "the same lines read again are news");
        assert_eq!(work.history.map(|h| h.asked), Some(2));
    }
}
