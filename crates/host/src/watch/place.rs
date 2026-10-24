//! The worktree in the middle of the window: its files, which `@` in the
//! prompt offers, and whether jj keeps it, read each time another worktree
//! comes to the middle or the window asks for everything read again.

use std::time::Duration;

use adroit_protocol::{PlaceChange, PlaceRead};
use adroit_vcs::Repo;

use super::{Watched, distinct};

pub(super) async fn watch(mut watched: Watched) {
    loop {
        for worktree in distinct(&watched.sights(), |s| s.place.clone()) {
            // Only the host's own machine can be read for now.
            let Some(path) = worktree.on_local() else {
                continue;
            };
            let repo = Repo::open(path);
            let read = PlaceRead {
                files: repo.files().await.unwrap_or_default(),
                jj: repo.is_jj(),
                worktree,
            };
            watched.record([PlaceChange::Read { read }]);
        }
        watched
            .wait(Duration::from_secs(300), |seen| {
                seen.iter()
                    .map(|s| s.sight.place.clone())
                    .collect::<Vec<_>>()
            })
            .await;
    }
}
