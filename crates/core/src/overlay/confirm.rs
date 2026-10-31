//! A question the window holds the keys for until it is answered.

use adroit_model::{EnvironmentId, Process, Session};

use crate::details::Entry;
use crate::icons::Icon;

#[derive(Debug, Clone, PartialEq)]
pub enum Ask {
    /// End the session, in the environment it runs in.
    Stop(EnvironmentId, Session),
    /// Kill the process of the session, in the environment it runs in.
    KillProcess(EnvironmentId, Session, Process),
    /// Restart the terminal host of the environment.
    RestartDaemon(EnvironmentId),
}

impl Ask {
    /// Kill the process the overview's row `entry` stands for, in
    /// `session` of `environment`; a process already gone is not asked of.
    pub fn kill_process(
        environment: &EnvironmentId,
        session: Option<&Session>,
        entry: Option<&Entry>,
    ) -> Option<Self> {
        match entry? {
            Entry::Process(process) if !process.zombie => Some(Self::KillProcess(
                environment.clone(),
                session?.clone(),
                process.clone(),
            )),
            _ => None,
        }
    }

    /// What the dialog shows: its icon, its title, what it says, and what
    /// answering yes is called.
    pub fn dialog(&self) -> (Icon, String, String, &'static str) {
        match self {
            Self::Stop(_, s) => (
                Icon::PlayerStop,
                format!("End {}?", s.name),
                "The agent exits where it runs. Its conversation remains in the provider's history.".into(),
                "End",
            ),
            Self::KillProcess(_, s, p) => (
                Icon::X,
                format!("Kill {}?", p.name),
                format!("Send SIGKILL to PID {} in session {}? Unsaved work in this process will be lost.", p.pid, s.name),
                "Kill process",
            ),
            Self::RestartDaemon(_) => (
                Icon::ServerBolt,
                "Restart the sessions' daemon?".into(),
                "Every session it holds ends: agents and shells, with what they scrolled back. Conversations remain in the provider's history.".into(),
                "Restart daemon",
            ),
        }
    }

    /// Whether answering yes loses what cannot be had back.
    pub fn destroys(&self) -> bool {
        matches!(self, Self::KillProcess(..) | Self::RestartDaemon(_))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn restarting_the_daemon_is_asked_as_a_loss_as_killing_a_process_is() {
        assert!(Ask::RestartDaemon(EnvironmentId::default()).destroys());
        assert!(!Ask::Stop(EnvironmentId::default(), Session::default()).destroys());
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
        let there = EnvironmentId::new("there");
        let ask = Ask::kill_process(
            &there,
            Some(&session),
            Some(&Entry::Process(process.clone())),
        )
        .unwrap();
        assert_eq!(
            ask,
            Ask::KillProcess(there.clone(), session.clone(), process.clone())
        );
        let (_, title, body, action) = ask.dialog();
        assert!(title.contains("node_repl"));
        assert!(body.contains("123"));
        assert!(body.contains("work"));
        assert!(body.contains("SIGKILL"));
        assert_eq!(action, "Kill process");
        assert!(Ask::kill_process(&there, None, Some(&Entry::Process(process.clone()))).is_none());
        assert!(Ask::kill_process(&there, Some(&session), None).is_none());
        process.zombie = true;
        assert!(
            Ask::kill_process(&there, Some(&session), Some(&Entry::Process(process))).is_none()
        );
    }
}
