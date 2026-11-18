//! What the agent of each session shown recorded of its work, while the
//! details are open, and of each other session whose timeline they list.

use std::collections::BTreeSet;
use std::time::Duration;

use adroit_protocol::DetailsChange;

use super::{Watched, shown_sessions};
use crate::work::Seen;

pub(super) async fn watch(mut watched: Watched) {
    loop {
        let root = crate::delegation::app::root();
        let seen = watched.seen();
        let shown = shown_sessions(&seen, |s| s.details)
            .into_iter()
            .map(|(s, _)| s);
        let others = watched.host().listed_among(&listed(&seen));
        let mut read = BTreeSet::new();
        for session in shown.chain(others) {
            if !read.insert(session.id.clone()) {
                continue;
            }
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
                let shown = seen
                    .iter()
                    .map(|s| (s.session.as_ref().map(|s| s.id.clone()), s.sight.details))
                    .collect::<Vec<_>>();
                (shown, listed(seen))
            })
            .await;
    }
}

/// The sessions whose timelines the open details list beside the one shown.
fn listed(seen: &[Seen]) -> BTreeSet<String> {
    seen.iter()
        .filter(|s| s.sight.details)
        .flat_map(|s| s.sight.timelines.iter().cloned())
        .collect()
}
