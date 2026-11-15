//! What the agent of each session shown recorded of its work, while the
//! details are open.

use std::time::Duration;

use adroit_protocol::DetailsChange;

use super::{Watched, shown_sessions};

pub(super) async fn watch(mut watched: Watched) {
    loop {
        let root = crate::delegation::app::root();
        for (session, _) in shown_sessions(&watched.seen(), |s| s.details) {
            let Some(code) = session.code.clone() else {
                continue;
            };
            if let Ok(entries) = crate::delegation::app::timeline_at(&root, code).await {
                watched.record([DetailsChange::Timeline {
                    session: session.id,
                    entries,
                }]);
            }
        }
        watched
            .wait(Duration::from_secs(1), |seen| {
                seen.iter()
                    .map(|s| (s.session.as_ref().map(|s| s.id.clone()), s.sight.details))
                    .collect::<Vec<_>>()
            })
            .await;
    }
}
