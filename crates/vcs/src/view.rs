//! What the details panel reads of a worktree: the files a change touches,
//! one file's diff in it, or the history.

use crate::{Error, Listing, Repo, Rev, Revision, Spot, git, jj};

/// How much is read for a spot.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Limits {
    /// The most lines of a diff.
    pub diff_lines: usize,
    /// The most commits or changes of the history.
    pub log_commits: usize,
}

/// What was read for a spot, and the revision whose files it lists.
#[derive(Debug)]
pub struct View {
    pub listing: Result<Listing, Error>,
    pub revision: Option<Revision>,
}

pub(crate) async fn load(repo: &Repo, spot: &Spot, limits: Limits) -> View {
    let worktree = repo.path();
    let mut listing = None;
    let mut read_with_jj = false;
    if repo.is_jj() {
        let rev = |r: &Rev| match r {
            Rev::WorkingCopy => "@".to_string(),
            Rev::Commit(id) => id.clone(),
        };
        let read = match spot {
            Spot::Files(r) => jj::files(worktree, &rev(r)).await.map(Listing::Files),
            Spot::Diff(r, c) => jj::file_diff(worktree, &rev(r), c, limits.diff_lines)
                .await
                .map(Listing::Diff),
            Spot::Log => jj::history(worktree, limits.log_commits)
                .await
                .map(Listing::Log),
        };
        // Without jj, a colocated repository still reads through git.
        if !matches!(read, Err(Error::Spawn { .. })) {
            read_with_jj = true;
            listing = Some(read);
        }
    }
    let read = match listing {
        Some(read) => read,
        None => {
            let rev = |r: &Rev| match r {
                Rev::WorkingCopy => None,
                Rev::Commit(id) => Some(id.clone()),
            };
            match spot {
                Spot::Files(Rev::WorkingCopy) => git::changes(worktree).await.map(Listing::Files),
                Spot::Files(Rev::Commit(id)) => {
                    git::commit_files(worktree, id).await.map(Listing::Files)
                }
                Spot::Diff(r, c) => {
                    git::file_diff(worktree, rev(r).as_deref(), c, limits.diff_lines)
                        .await
                        .map(Listing::Diff)
                }
                Spot::Log => git::history(worktree, limits.log_commits)
                    .await
                    .map(Listing::Log),
            }
        }
    };
    let revision = if read.is_ok() {
        match spot {
            Spot::Files(rev) => {
                let target = match rev {
                    Rev::WorkingCopy if read_with_jj => "@",
                    Rev::WorkingCopy => "HEAD",
                    Rev::Commit(id) => id,
                };
                if read_with_jj {
                    jj::revision(worktree, target).await.ok()
                } else {
                    git::revision(worktree, target).await.ok()
                }
            }
            _ => None,
        }
    } else {
        None
    };
    View {
        listing: read,
        revision,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const LIMITS: Limits = Limits {
        diff_lines: 2000,
        log_commits: 50,
    };

    #[tokio::test]
    async fn changes_and_opened_log_entries_show_their_full_descriptions_even_without_files() {
        let dir = tempfile::tempdir().unwrap();
        let jj = |args: &[&str]| {
            std::process::Command::new("jj")
                .args([
                    "--config",
                    "user.name=test",
                    "--config",
                    "user.email=test@example.com",
                ])
                .args(args)
                .current_dir(dir.path())
                .output()
                .unwrap()
                .status
                .success()
        };
        assert!(jj(&["git", "init", "."]));
        assert!(jj(&[
            "describe",
            "-m",
            "first subject",
            "-m",
            "The reason for this change."
        ]));
        let repo = Repo::open(dir.path());
        let changes = repo.load(&Spot::Files(Rev::WorkingCopy), LIMITS).await;
        assert!(matches!(changes.listing, Ok(Listing::Files(ref files)) if files.is_empty()));
        let first = changes.revision.unwrap();
        assert_eq!(
            first.description,
            "first subject\n\nThe reason for this change."
        );
        assert!(jj(&["new", "-m", "second subject"]));
        let opened = repo
            .load(&Spot::Files(Rev::Commit(first.id.clone())), LIMITS)
            .await;
        assert_eq!(opened.revision, Some(first));
        let changes = repo.load(&Spot::Files(Rev::WorkingCopy), LIMITS).await;
        assert_eq!(changes.revision.unwrap().description, "second subject");
        assert!(repo.load(&Spot::Log, LIMITS).await.revision.is_none());
    }

    #[tokio::test]
    async fn an_unborn_git_worktree_lists_its_files_without_a_commit_description() {
        let dir = tempfile::tempdir().unwrap();
        assert!(
            std::process::Command::new("git")
                .args(["init", "-q"])
                .current_dir(dir.path())
                .status()
                .unwrap()
                .success()
        );
        std::fs::write(dir.path().join("new.txt"), "new").unwrap();
        let view = Repo::open(dir.path())
            .load(&Spot::Files(Rev::WorkingCopy), LIMITS)
            .await;
        assert!(matches!(view.listing, Ok(Listing::Files(ref files)) if files.len() == 1));
        assert!(view.revision.is_none());
    }
}
