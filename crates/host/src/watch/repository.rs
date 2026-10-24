//! A worktree's repository, as the details panel's changes and log list it:
//! read only while one of those is shown, since every jj command may record
//! the working copy.

use std::time::Duration;

use adroit_protocol::DetailsChange;

use super::{Watched, distinct};

pub(super) async fn watch(mut watched: Watched) {
    loop {
        for shown in distinct(&watched.sights(), |s| s.repository.clone()) {
            let read = crate::details::load(shown.spot, &shown.worktree).await;
            watched.record([DetailsChange::Repository {
                worktree: shown.worktree,
                read,
            }]);
        }
        watched
            .wait(Duration::from_secs(2), |seen| {
                seen.iter()
                    .map(|s| s.sight.repository.clone())
                    .collect::<Vec<_>>()
            })
            .await;
    }
}
