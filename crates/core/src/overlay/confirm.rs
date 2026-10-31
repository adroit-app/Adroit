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
    /// Remove the environment, by this name, from the environments kept.
    RemoveEnvironment(EnvironmentId, String),
    /// Take the workspace, by this name, out of the tree by ending what runs
    /// there: these sessions and the terminals by these ids.
    RemoveWorkspace {
        environment: EnvironmentId,
        name: String,
        sessions: Vec<Session>,
        terminals: Vec<u128>,
    },
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
            Self::RemoveEnvironment(_, name) => (
                Icon::SquareMinus,
                format!("Remove {name}?"),
                "Every window stops listing it. Its sessions keep running on its host, and it can be restored from Settings.".into(),
                "Remove",
            ),
            Self::RemoveWorkspace {
                name,
                sessions,
                terminals,
                ..
            } => {
                let running = [
                    counted(sessions.len(), "session"),
                    counted(terminals.len(), "terminal"),
                ]
                .into_iter()
                .flatten()
                .collect::<Vec<_>>()
                .join(" and ");
                (
                    Icon::SquareMinus,
                    format!("Remove {name} from the tree?"),
                    format!("Ending its {running} takes it out of the tree; it is listed again once anything starts there. Conversations remain in the provider's history."),
                    "End and remove",
                )
            }
        }
    }

    /// Whether answering yes loses what cannot be had back.
    pub fn destroys(&self) -> bool {
        match self {
            Self::KillProcess(..) | Self::RestartDaemon(_) => true,
            Self::RemoveWorkspace { terminals, .. } => !terminals.is_empty(),
            Self::Stop(..) | Self::RemoveEnvironment(..) => false,
        }
    }
}

/// `n` of `what`, said as a count; none when there are none.
fn counted(n: usize, what: &str) -> Option<String> {
    match n {
        0 => None,
        1 => Some(format!("1 {what}")),
        n => Some(format!("{n} {what}s")),
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
    fn removing_an_environment_names_it_and_is_not_asked_as_a_loss() {
        let ask = Ask::RemoveEnvironment(EnvironmentId::new("there"), "There".into());
        let (_, title, body, action) = ask.dialog();
        assert_eq!(title, "Remove There?");
        assert!(body.contains("keep running"), "{body}");
        assert_eq!(action, "Remove");
        assert!(!ask.destroys());
    }

    fn removing(sessions: Vec<Session>, terminals: Vec<u128>) -> Ask {
        Ask::RemoveWorkspace {
            environment: EnvironmentId::default(),
            name: "api".into(),
            sessions,
            terminals,
        }
    }

    #[test]
    fn removing_a_workspace_names_it_and_counts_what_ends_there() {
        let (_, title, body, action) =
            removing(vec![Session::default(), Session::default()], vec![7]).dialog();
        assert_eq!(title, "Remove api from the tree?");
        assert!(
            body.starts_with("Ending its 2 sessions and 1 terminal takes"),
            "{body}"
        );
        assert_eq!(action, "End and remove");
        let (_, _, body, _) = removing(vec![Session::default()], vec![]).dialog();
        assert!(body.starts_with("Ending its 1 session takes"), "{body}");
    }

    #[test]
    fn removing_a_workspace_is_asked_as_a_loss_only_when_terminals_end_with_it() {
        assert!(!removing(vec![Session::default()], vec![]).destroys());
        assert!(removing(vec![], vec![7]).destroys());
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
