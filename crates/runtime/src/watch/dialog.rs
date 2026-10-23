//! Whether the session the window's console shows has a dialog of its
//! agent's own open, such as the picker `/model` opens, which takes every key:
//! judged by the agent's adapter from the session's screen, read again each
//! time the window draws it anew. An agent reports no status for a dialog it
//! opens itself, so only its screen tells.

use std::time::Duration;

use adroit_model::Agent;
use adroit_model::daemon::id_of;
use adroit_protocol::{ConsoleChange, Dialog};

use super::Watched;

/// How long the daemon is given to show a screen.
const VISIBLE_WAIT: Duration = Duration::from_millis(300);

pub(super) async fn watch(mut watched: Watched) {
    let daemon = crate::daemon::daemon();
    while let Some((session, agent)) = watched.redrawn().await {
        let Some(id) = id_of(&session) else {
            continue;
        };
        let Ok(Ok(lines)) = tokio::time::timeout(VISIBLE_WAIT, daemon.visible(id)).await else {
            continue;
        };
        let open = dialog_open(agent, &lines);
        watched.record([ConsoleChange::Dialog {
            dialog: Dialog { session, open },
        }]);
    }
}

/// Whether `lines`, a screen of a session of `agent`, show a dialog of the
/// agent's own.
fn dialog_open(agent: Agent, lines: &[String]) -> bool {
    adroit_agents::adapter(agent).dialog_open(lines)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn claude_screen(name: &str) -> Vec<String> {
        let path = format!(
            "{}/../agents/tests/screens/{name}.txt",
            env!("CARGO_MANIFEST_DIR")
        );
        std::fs::read_to_string(&path)
            .unwrap_or_else(|e| panic!("{path}: {e}"))
            .lines()
            .map(String::from)
            .collect()
    }

    #[test]
    fn a_screen_is_judged_by_the_adapter_of_the_agent_the_session_runs() {
        assert!(dialog_open(Agent::Claude, &claude_screen("model")));
        assert!(!dialog_open(Agent::Claude, &claude_screen("input")));
        assert!(!dialog_open(Agent::Codex, &claude_screen("input")));
    }
}
