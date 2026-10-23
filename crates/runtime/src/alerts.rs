//! Telling the user, outside the window, that a session waits on them.

use adroit_model::{State, Tree};

/// The sessions waiting in `next` that were not waiting in `prev`, each as
/// its name and what it waits for.
pub fn newly_waiting(prev: &Tree, next: &Tree) -> Vec<(String, String)> {
    let waited = |id: &str| {
        prev.sessions()
            .any(|(_, s)| s.id == id && matches!(s.state, State::Waiting(_)))
    };
    next.sessions()
        .filter_map(|(_, s)| match &s.state {
            State::Waiting(why) if !waited(&s.id) => Some((s.name.clone(), why.to_string())),
            _ => None,
        })
        .collect()
}

/// The AppleScript that shows a notification, its words quoted so none can
/// end the string they are in.
fn script(title: &str, subtitle: &str, body: &str) -> String {
    let q = |s: &str| format!("\"{}\"", s.replace('\\', "\\\\").replace('"', "\\\""));
    format!(
        "display notification {} with title {} subtitle {}",
        q(body),
        q(title),
        q(subtitle)
    )
}

/// Show a notification that the session `name` waits, for `why`, once the
/// future it gives is run.
pub fn notify_waiting(name: &str, why: &str) -> impl std::future::Future<Output = ()> + use<> {
    let mut cmd = tokio::process::Command::new("osascript");
    cmd.args(["-e", &script("Adroit", name, why)])
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null());
    async move {
        let _ = cmd.status().await;
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use adroit_model::{Kind, Session, State, Tree, Wait, build};

    use super::*;

    fn tree(states: &[(&str, State)]) -> Tree {
        let sessions: Vec<Session> = states
            .iter()
            .map(|(id, state)| Session {
                id: id.to_string(),
                name: format!("{id}-name"),
                cwd: "/tmp".into(),
                kind: Kind::Own,
                state: state.clone(),
                ..Default::default()
            })
            .collect();
        build(&sessions, &[], &HashMap::new(), &[], None)
    }

    #[test]
    fn only_a_session_that_starts_waiting_is_told_of() {
        let before = tree(&[("a", State::Working), ("b", State::Waiting(Wait::Input))]);
        let after = tree(&[
            ("a", State::Waiting(Wait::Permission)),
            ("b", State::Waiting(Wait::Input)),
            ("c", State::Waiting(Wait::Input)),
        ]);
        assert_eq!(
            newly_waiting(&before, &after),
            [
                ("a-name".to_string(), "permission prompt".to_string()),
                ("c-name".to_string(), "waiting for input".to_string()),
            ]
        );
        assert!(newly_waiting(&after, &after).is_empty());
    }

    #[test]
    fn a_notification_s_words_cannot_break_out_of_the_script() {
        assert_eq!(
            script("Adroit", "say \"hi\"", "a\\b"),
            r#"display notification "a\\b" with title "Adroit" subtitle "say \"hi\"""#
        );
    }
}
