//! The conversations every agent kept in each connection, listed while the
//! window shows the list of them: read again each time it changes which directory it lists, or
//! asks for everything read again, and every half minute meanwhile.

use std::time::Duration;

use adroit_model::newest_first;
use adroit_protocol::PastChange;

use super::{Watched, distinct};

pub(super) async fn watch(mut watched: Watched) {
    loop {
        for sight in distinct(&watched.sights(), |s| s.past.clone()) {
            let dir = sight.dir.clone();
            let read = tokio::task::spawn_blocking(move || {
                let (settings, home) = (crate::settings::current(), crate::paths::home());
                let pasts = crate::programs::connections(&settings)
                    .iter()
                    .flat_map(|c| adroit_agents::adapter(c.agent()).past(c, &home, dir.as_deref()))
                    .collect();
                newest_first(pasts)
            })
            .await;
            if let Ok(conversations) = read {
                watched.record([PastChange::Listed {
                    sight,
                    conversations,
                }]);
            }
        }
        watched
            .wait(Duration::from_secs(30), |seen| {
                seen.iter()
                    .map(|s| s.sight.past.clone())
                    .collect::<Vec<_>>()
            })
            .await;
    }
}
