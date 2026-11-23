//! Answering what a session asks the user from a client that does not draw
//! its terminal: the keys for the answer are typed into the session's
//! terminal only while the session still asks exactly what was answered and
//! its screen shows that very dialog, so a key never lands on a dialog
//! drawn after it.

use std::time::Duration;

use adroit_model::Agent;
use adroit_model::asking::{Asking, Reply};
use adroit_protocol::{Answer, Typing};

use crate::{asks, daemon, hooked, sessions};

/// How long the terminal host is given to show the session's screen.
const VISIBLE_WAIT: Duration = Duration::from_millis(300);

/// One answer at a time across every session, so that two clients answering
/// the same question cannot both pass the check before either has typed.
static ANSWERING: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

/// Give `reply` to `asking`, which the session `session` asks.
pub(crate) async fn answer(
    session: &str,
    asking: &Asking,
    reply: &Reply,
) -> Result<Answer, String> {
    let id = sessions::id_of(session).ok_or("invalid session ID")?;
    let _turn = ANSWERING.lock().await;
    let daemon = daemon::daemon();
    let listed = daemon.sessions().await.map_err(|e| e.to_string())?;
    let actual = sessions::agents(&listed)
        .find(|s| s.id == session)
        .ok_or("the session no longer exists")?;
    let record = actual
        .delegation
        .ok_or("Adroit hears nothing this session asks")?;
    let screen = match tokio::time::timeout(VISIBLE_WAIT, daemon.visible(id)).await {
        Ok(Ok(lines)) => lines,
        _ => return Err("the session's screen cannot be read".into()),
    };
    let keys = keys(
        hooked::asks(record).as_ref(),
        asking,
        actual.agent,
        reply,
        &screen,
    )?;
    let typed = asks::type_prompt(id, &[Typing::Text { text: keys }]).await;
    // Typed in part or whole, the dialog may have taken it, so it is not
    // offered for answering again.
    hooked::answered(record, asking);
    match typed {
        Ok(()) => Ok(Answer::Done),
        Err(text) => Ok(Answer::Unconfirmed { text }),
    }
}

/// The keys that give `reply` to `answered` for a session of `agent` that
/// asks `now` and shows `screen`, or why none may be typed.
fn keys(
    now: Option<&Asking>,
    answered: &Asking,
    agent: Agent,
    reply: &Reply,
    screen: &[String],
) -> Result<String, String> {
    if now != Some(answered) {
        return Err("the session no longer asks that".into());
    }
    adroit_agents::answers::keys(agent, &answered.asked, reply, screen)
}

#[cfg(test)]
mod tests {
    use adroit_model::asking::Asked;

    use super::*;

    fn bash(since_ms: u64, about: &str) -> Asking {
        Asking {
            since_ms,
            asked: Asked::Tool {
                name: "Bash".into(),
                about: about.into(),
            },
        }
    }

    fn permission() -> Vec<String> {
        let path = format!(
            "{}/../agents/tests/screens/permission.txt",
            env!("CARGO_MANIFEST_DIR")
        );
        std::fs::read_to_string(&path)
            .unwrap_or_else(|e| panic!("{path}: {e}"))
            .lines()
            .map(String::from)
            .collect()
    }

    #[test]
    fn what_the_session_still_asks_is_answered_by_the_keys_its_dialog_takes() {
        let asked = bash(10, "date > stamp2.txt");
        let keys = keys(
            Some(&asked),
            &asked,
            Agent::Claude,
            &Reply::Allow,
            &permission(),
        );
        assert_eq!(keys.as_deref(), Ok("1"));
    }

    #[test]
    fn a_question_asked_again_since_is_not_answered_by_an_answer_to_the_one_before() {
        let (before, since) = (bash(10, "date > stamp2.txt"), bash(20, "date > stamp2.txt"));
        let keys = keys(
            Some(&since),
            &before,
            Agent::Claude,
            &Reply::Allow,
            &permission(),
        );
        assert!(keys.is_err(), "{keys:?}");
    }

    #[test]
    fn a_session_that_asks_nothing_now_is_not_answered() {
        let asked = bash(10, "date > stamp2.txt");
        let keys = keys(None, &asked, Agent::Claude, &Reply::Allow, &permission());
        assert!(keys.is_err(), "{keys:?}");
    }
}
