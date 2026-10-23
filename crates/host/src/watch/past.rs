//! The conversations every agent kept, listed while the window shows the list
//! of them: read again each time it changes which directory it lists, or
//! asks for everything read again, and every half minute meanwhile.

use std::time::Duration;

use adroit_model::{Agent, newest_first};
use adroit_protocol::PastChange;

use super::Watched;

pub(super) async fn watch(mut watched: Watched) {
    loop {
        if let Some(sight) = watched.sight().past {
            let dir = sight.dir.clone();
            let read = tokio::task::spawn_blocking(move || {
                let (settings, home) = (crate::programs::agents(), crate::paths::home());
                let pasts = Agent::ALL
                    .iter()
                    .flat_map(|a| adroit_agents::adapter(*a).past(&settings, &home, dir.as_deref()))
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
            .wait(Duration::from_secs(30), |s| s.sight.past.clone())
            .await;
    }
}
