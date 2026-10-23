//! Reading what a spot of the details panel shows.

use std::path::PathBuf;

use adroit_vcs::{Limits, Repo, Spot};

pub use adroit_core::details::*;

/// Read what `spot` shows of `worktree`: through jj in a jj repository,
/// through git otherwise or when jj cannot be run.
pub async fn load(spot: Spot, worktree: PathBuf) -> View {
    let details = crate::settings::current().details;
    let limits = Limits {
        diff_lines: details.diff_lines,
        log_commits: details.log_commits,
    };
    let read = Repo::open(&worktree).load(&spot, limits).await;
    let listing = match read.listing {
        Ok(adroit_vcs::Listing::Files(files)) => Listing::Files(files),
        Ok(adroit_vcs::Listing::Log(commits)) => Listing::Log(commits),
        Ok(adroit_vcs::Listing::Diff(lines)) => Listing::Diff(lines),
        Err(e) => Listing::Error(e.to_string()),
    };
    View {
        spot,
        worktree,
        listing,
        revision: read.revision,
    }
}
