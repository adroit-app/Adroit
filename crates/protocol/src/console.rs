//! What the host sees on the screen of the session a window's console
//! shows: whether a dialog of its agent's own is open there, which takes
//! every key.

use serde::{Deserialize, Serialize};

use crate::work::set;

/// Whether the screen of `session` showed a dialog of its agent's own when
/// the host last looked.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Dialog {
    pub session: String,
    pub open: bool,
}

/// What the host saw on the console's screen.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ConsoleWork {
    /// The session whose screen was looked at last; none before one is.
    pub dialog: Option<Dialog>,
}

/// One change to what the host saw on the console's screen.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ConsoleChange {
    Dialog { dialog: Dialog },
}

impl ConsoleWork {
    /// What was seen with `change` made; whether that changed it.
    pub fn apply(&mut self, change: ConsoleChange) -> bool {
        let ConsoleChange::Dialog { dialog } = change;
        set(&mut self.dialog, Some(dialog))
    }

    /// Whether `session` showed a dialog of its agent's own when last
    /// looked at.
    pub fn dialog_open(&self, session: &str) -> bool {
        self.dialog
            .as_ref()
            .is_some_and(|d| d.session == session && d.open)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn seen(session: &str, open: bool) -> ConsoleChange {
        ConsoleChange::Dialog {
            dialog: Dialog {
                session: session.into(),
                open,
            },
        }
    }

    #[test]
    fn the_screen_looked_at_last_says_whether_its_session_shows_a_dialog() {
        let mut work = ConsoleWork::default();
        assert!(!work.dialog_open("a"));
        assert!(work.apply(seen("a", true)));
        assert!(!work.apply(seen("a", true)));
        assert!(work.dialog_open("a"));
        assert!(!work.dialog_open("b"));
        assert!(work.apply(seen("b", false)));
        assert!(!work.dialog_open("a"), "another screen was looked at since");
    }
}
