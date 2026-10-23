//! What jj or git says of the files of each worktree whose files the tree
//! lists, read again every two seconds so the marks follow the disk.

use std::time::Duration;

use adroit_protocol::TreeChange;
use adroit_vcs::Repo;

use super::Watched;

pub(super) async fn watch(mut watched: Watched) {
    loop {
        let mut read = Vec::new();
        for worktree in watched.sight().tree.marked {
            let marks = Repo::open(&worktree).marks().await;
            read.push(TreeChange::Marks { worktree, marks });
        }
        watched.record(read);
        watched
            .wait(Duration::from_secs(2), |s| s.tree.marked.clone())
            .await;
    }
}
