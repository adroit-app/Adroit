//! The live processes of each session shown, for its overview.

use std::collections::HashMap;
use std::time::Duration;

use adroit_protocol::DetailsChange;

use super::{Watched, shown_sessions};
use crate::processes::Sampler;

pub(super) async fn watch(mut watched: Watched) {
    // One for each session, as a sampler measures from its last sample.
    let mut samplers: HashMap<String, Sampler> = HashMap::new();
    loop {
        let shown = shown_sessions(&watched.seen(), |s| s.details && s.overview);
        samplers.retain(|id, _| shown.iter().any(|(s, _)| s.id == *id));
        for (session, _) in shown {
            let id = session.id.clone();
            let mut sampler = samplers.remove(&id).unwrap_or_default();
            if let Ok((next, processes)) = tokio::task::spawn_blocking(move || {
                let rows = sampler.sample(&session);
                (sampler, rows)
            })
            .await
            {
                samplers.insert(id.clone(), next);
                watched.record([DetailsChange::Processes {
                    session: id,
                    processes,
                }]);
            }
        }
        watched
            .wait(Duration::from_secs(2), |seen| {
                seen.iter()
                    .map(|s| {
                        (
                            s.session.as_ref().map(|s| s.id.clone()),
                            s.sight.overview,
                            s.sight.details,
                        )
                    })
                    .collect::<Vec<_>>()
            })
            .await;
    }
}
