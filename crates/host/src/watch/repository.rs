//! A worktree's repository, as the details panel's changes and log list it:
//! read only while one of those is shown, since every jj command may record
//! the working copy.

use std::time::Duration;

use adroit_protocol::{DetailsChange, RepositorySight};

use super::Watched;

pub(super) async fn watch(mut watched: Watched) {
    loop {
        let mut spots: Vec<RepositorySight> = Vec::new();
        for spot in watched.sights().into_iter().flat_map(|s| s.repositories) {
            if !spots.contains(&spot) {
                spots.push(spot);
            }
        }
        for shown in spots {
            let read = crate::details::load(shown.spot, &shown.worktree).await;
            watched.record([DetailsChange::Repository {
                worktree: shown.worktree,
                read,
            }]);
        }
        watched
            .wait(Duration::from_secs(2), |seen| {
                seen.iter()
                    .map(|s| s.sight.repositories.clone())
                    .collect::<Vec<_>>()
            })
            .await;
    }
}
