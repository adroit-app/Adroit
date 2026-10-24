//! A question the window holds the keys for until it is answered.

use adroit_model::{Process, Session};

use crate::details::Entry;
use crate::icons::Icon;

#[derive(Debug, Clone, PartialEq)]
pub enum Ask {
    Stop(Session),
    KillProcess(Session, Process),
    RestartDaemon,
}

impl Ask {
    /// Kill the process the overview's row `entry` stands for, in
    /// `session`; a process already gone is not asked of.
    pub fn kill_process(session: Option<&Session>, entry: Option<&Entry>) -> Option<Self> {
        match entry? {
            Entry::Process(process) if !process.zombie => {
                Some(Self::KillProcess(session?.clone(), process.clone()))
            }
            _ => None,
        }
    }

    /// What the dialog shows: its icon, its title, what it says, and what
    /// answering yes is called.
    pub fn dialog(&self) -> (Icon, String, String, &'static str) {
        match self {
            Self::Stop(s) => (
                Icon::PlayerStop,
                format!("End {}?", s.name),
                "The agent exits where it runs. Its conversation remains in the provider's history.".into(),
                "End",
            ),
            Self::KillProcess(s, p) => (
                Icon::X,
                format!("Kill {}?", p.name),
                format!("Send SIGKILL to PID {} in session {}? Unsaved work in this process will be lost.", p.pid, s.name),
                "Kill process",
            ),
            Self::RestartDaemon => (
                Icon::ServerBolt,
                "Restart the sessions' daemon?".into(),
                "Every session it holds ends: agents and shells, with what they scrolled back. Conversations remain in the provider's history.".into(),
                "Restart daemon",
            ),
        }
    }

    /// Whether answering yes loses what cannot be had back.
    pub fn destroys(&self) -> bool {
        matches!(self, Self::KillProcess(..) | Self::RestartDaemon)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn restarting_the_daemon_is_asked_as_a_loss_as_killing_a_process_is() {
        assert!(Ask::RestartDaemon.destroys());
        assert!(!Ask::Stop(Session::default()).destroys());
    }

    #[test]
    fn killing_an_overview_process_requires_a_confirmation_bound_to_its_session_and_identity() {
        let session = adroit_model::testing::session("session-one")
            .name("work")
            .build();
        let mut process = Process {
            pid: 123,
            started_secs: 456,
            name: "node_repl".into(),
            ..Default::default()
        };
        let ask =
            Ask::kill_process(Some(&session), Some(&Entry::Process(process.clone()))).unwrap();
        assert_eq!(ask, Ask::KillProcess(session.clone(), process.clone()));
        let (_, title, body, action) = ask.dialog();
        assert!(title.contains("node_repl"));
        assert!(body.contains("123"));
        assert!(body.contains("work"));
        assert!(body.contains("SIGKILL"));
        assert_eq!(action, "Kill process");
        assert!(Ask::kill_process(None, Some(&Entry::Process(process.clone()))).is_none());
        assert!(Ask::kill_process(Some(&session), None).is_none());
        process.zombie = true;
        assert!(Ask::kill_process(Some(&session), Some(&Entry::Process(process))).is_none());
    }
}
