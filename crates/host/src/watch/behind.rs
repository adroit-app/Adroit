//! Whether each session has a subagent still running, followed in its
//! transcript from one read of the sessions to the next.

use std::collections::HashMap;
use std::sync::{LazyLock, Mutex};

use adroit_agents::WorkBehind;
use adroit_model::Session;

/// A session's follower, with the conversation it follows: the session
/// may go on in another.
type Held = (String, Box<dyn WorkBehind>);

/// The follower of each session, by its id in the terminal host.
static HELD: LazyLock<Mutex<HashMap<u128, Held>>> = LazyLock::new(Default::default);

/// Whether a subagent the session `id`, listed as `session`, started still
/// runs; false for one whose agent writes no such work down.
pub(crate) async fn running(id: u128, session: &Session) -> bool {
    if session.session_id.is_empty() {
        return false;
    }
    let held = HELD
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .remove(&id)
        .filter(|(conversation, _)| *conversation == session.session_id);
    let follower = match held {
        Some((_, follower)) => follower,
        None => {
            let Ok(context) = crate::programs::of_session(session).await else {
                return false;
            };
            let Some(follower) = adroit_agents::adapter(session.agent).work_behind(
                &context,
                &crate::paths::home(),
                session,
            ) else {
                return false;
            };
            follower
        }
    };
    let Ok((follower, running)) = crate::environment::spawn_blocking(move || {
        let mut follower = follower;
        let running = follower.running();
        (follower, running)
    })
    .await
    else {
        return false;
    };
    HELD.lock()
        .unwrap_or_else(|e| e.into_inner())
        .insert(id, (session.session_id.clone(), follower));
    running
}

/// Forgets every session but those `kept`.
pub(crate) fn keep_only(kept: &[u128]) {
    let mut held = HELD.lock().unwrap_or_else(|e| e.into_inner());
    held.retain(|id, _| kept.contains(id));
}
