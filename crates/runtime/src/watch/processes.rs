//! The live processes of the session shown, for its overview.

use std::time::Duration;

use adroit_protocol::DetailsChange;

use super::Watched;
use crate::processes::Sampler;

pub(super) async fn watch(mut watched: Watched) {
    let mut sampler = Sampler::default();
    loop {
        let sight = watched.sight();
        let shown = sight.session.filter(|_| sight.details && sight.overview);
        if let Some(session) = shown {
            let id = session.id.clone();
            match tokio::task::spawn_blocking(move || {
                let rows = sampler.sample(&session);
                (sampler, rows)
            })
            .await
            {
                Ok((next, processes)) => {
                    sampler = next;
                    watched.record([DetailsChange::Processes {
                        session: id,
                        processes,
                    }]);
                }
                Err(_) => sampler = Sampler::default(),
            }
        }
        watched
            .wait(Duration::from_secs(2), |s| {
                (
                    s.session.as_ref().map(|s| s.id.clone()),
                    s.overview,
                    s.details,
                )
            })
            .await;
    }
}
