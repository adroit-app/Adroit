//! What each directory the tree lists holds, read again every two seconds
//! so files made or removed meanwhile show.

use std::time::Duration;

use adroit_core::files;
use adroit_protocol::TreeChange;

use super::Watched;

pub(super) async fn watch(mut watched: Watched) {
    loop {
        let dirs = watched.sight().tree.listed;
        let read = tokio::task::spawn_blocking(move || {
            dirs.into_iter()
                .map(|dir| {
                    let entries = files::read(&dir);
                    TreeChange::Entries { dir, entries }
                })
                .collect::<Vec<_>>()
        })
        .await;
        if let Ok(read) = read {
            watched.record(read);
        }
        watched
            .wait(Duration::from_secs(2), |s| s.tree.listed.clone())
            .await;
    }
}
