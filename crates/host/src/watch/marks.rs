//! What jj or git says of the files of each worktree whose files the tree
//! lists, read again every two seconds so the marks follow the disk.

use std::time::Duration;

use adroit_protocol::TreeChange;
use adroit_vcs::Repo;

use std::collections::BTreeSet;
use std::path::PathBuf;

use super::Watched;
use crate::work::Seen;

pub(super) async fn watch(mut watched: Watched) {
    loop {
        let mut read = Vec::new();
        for worktree in marked(&watched.seen()) {
            let marks = Repo::open(&worktree).marks().await;
            read.push(TreeChange::Marks { worktree, marks });
        }
        watched.record(read);
        watched.wait(Duration::from_secs(2), marked).await;
    }
}

/// Every worktree whose files a window lists.
fn marked(seen: &[Seen]) -> BTreeSet<PathBuf> {
    seen.iter()
        .flat_map(|s| s.sight.tree.marked.iter().cloned())
        .collect()
}
