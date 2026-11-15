//! What GitHub holds open of each repository the tree lists: which of them
//! are on GitHub, asked of git now and then, and the listings a window has
//! open, asked of `gh` once a minute while they stay open.

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::path::PathBuf;
use std::time::{Duration, Instant};

use adroit_model::Tree;
use adroit_model::github::{Listing, Read, Repository};
use adroit_protocol::TreeChange;
use adroit_vcs::github;

use super::Watched;
use crate::work::Seen;

/// How long what was read of GitHub stands before it is read again.
const STANDS: Duration = Duration::from_secs(60);

/// How often the watcher looks whether a listing was opened or what it read
/// has stood long enough.
const LOOKS: Duration = Duration::from_secs(5);

pub(super) async fn watch(mut watched: Watched) {
    let mut kept = Kept::default();
    loop {
        let roots = repositories(watched.host().work().tree().tree());
        let open = open(&watched.seen());
        let now = Instant::now();
        if kept
            .gh
            .is_none_or(|(_, at)| now.duration_since(at) >= STANDS)
        {
            kept.gh = Some((github::installed().await, now));
        }
        let installed = kept.gh.is_some_and(|(installed, _)| installed);
        for root in &roots {
            let stale = kept
                .remotes
                .get(root)
                .is_none_or(|(_, at)| now.duration_since(*at) >= STANDS);
            if stale {
                let remote = github::remote(root).await;
                kept.remotes.insert(root.clone(), (remote, now));
            }
        }
        kept.remotes.retain(|root, _| roots.contains(root));
        for (root, listing) in due(&open, &kept.read, &kept.open, now) {
            let Some((Some(remote), _)) = kept.remotes.get(&root) else {
                continue;
            };
            if !installed {
                continue;
            }
            let read = github::list(&root, &remote.slug, listing).await;
            kept.read.insert((root, listing), (read, Instant::now()));
        }
        kept.read.retain(|(root, _), _| roots.contains(root));
        kept.open = open;
        watched.record(kept.changes(installed, &roots));
        watched.wait(LOOKS, self::open).await;
    }
}

/// What the watcher holds between its looks.
#[derive(Default)]
struct Kept {
    /// Whether `gh` can be run, and when that was asked.
    gh: Option<(bool, Instant)>,
    /// Each repository's remote on GitHub, if it has one, and when git was
    /// asked.
    remotes: HashMap<PathBuf, (Option<github::Remote>, Instant)>,
    /// What was last read of each listing, and when.
    read: HashMap<(PathBuf, Listing), (Read, Instant)>,
    /// The listings open at the last look.
    open: BTreeSet<(PathBuf, Listing)>,
    /// What was last recorded, so a repository gone is said to be.
    told: BTreeMap<PathBuf, bool>,
}

impl Kept {
    /// What every window is told of `roots`: each on GitHub with what was
    /// read of it, while `gh` can be run; and each no longer listed, or no
    /// longer on GitHub, as none.
    fn changes(&mut self, installed: bool, roots: &BTreeSet<PathBuf>) -> Vec<TreeChange> {
        let mut changes = Vec::new();
        for root in roots {
            let slug = self
                .remotes
                .get(root)
                .and_then(|(remote, _)| remote.as_ref())
                .filter(|_| installed)
                .map(|r| r.slug.clone());
            let Some(slug) = slug else {
                if self.told.remove(root).is_some() {
                    changes.push(TreeChange::GitHub {
                        repository: root.clone(),
                        github: None,
                    });
                }
                continue;
            };
            let mut repository = Repository::new(slug);
            for listing in Listing::ALL {
                *repository.read_mut(listing) = self
                    .read
                    .get(&(root.clone(), listing))
                    .map(|(read, _)| read.clone());
            }
            self.told.insert(root.clone(), true);
            changes.push(TreeChange::GitHub {
                repository: root.clone(),
                github: Some(Box::new(repository)),
            });
        }
        let gone: Vec<PathBuf> = self
            .told
            .keys()
            .filter(|root| !roots.contains(*root))
            .cloned()
            .collect();
        for root in gone {
            self.told.remove(&root);
            changes.push(TreeChange::GitHub {
                repository: root,
                github: None,
            });
        }
        changes
    }
}

/// The roots of the git repositories `tree` lists.
fn repositories(tree: &Tree) -> BTreeSet<PathBuf> {
    tree.workspaces
        .iter()
        .filter(|ws| ws.git && !ws.scratch)
        .map(|ws| ws.root.clone())
        .collect()
}

/// Every listing a window has open.
fn open(seen: &[Seen]) -> BTreeSet<(PathBuf, Listing)> {
    seen.iter()
        .flat_map(|s| s.sight.tree.github.iter().cloned())
        .collect()
}

/// The listings of `open` to read now: those opened since the last look,
/// those never read, and those whose read has stood its while.
fn due(
    open: &BTreeSet<(PathBuf, Listing)>,
    read: &HashMap<(PathBuf, Listing), (Read, Instant)>,
    was_open: &BTreeSet<(PathBuf, Listing)>,
    now: Instant,
) -> Vec<(PathBuf, Listing)> {
    open.iter()
        .filter(|l| {
            !was_open.contains(*l)
                || read
                    .get(*l)
                    .is_none_or(|(_, at)| now.duration_since(*at) >= STANDS)
        })
        .cloned()
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn listing(root: &str, listing: Listing) -> (PathBuf, Listing) {
        (root.into(), listing)
    }

    #[test]
    fn a_listing_is_read_once_opened_and_again_once_its_read_has_stood_a_minute() {
        let start = Instant::now();
        let pulls = listing("/r", Listing::Pulls);
        let open = BTreeSet::from([pulls.clone()]);
        let mut read = HashMap::new();
        // Opened at this look: read, whatever was read before.
        assert_eq!(
            due(&open, &read, &BTreeSet::new(), start),
            vec![pulls.clone()]
        );
        read.insert(pulls.clone(), (Ok(Vec::new()), start));
        assert_eq!(
            due(&open, &read, &BTreeSet::new(), start),
            vec![pulls.clone()],
            "opened again"
        );
        // Open since the last look: read only once it stood.
        assert!(due(&open, &read, &open, start + LOOKS).is_empty());
        assert_eq!(due(&open, &read, &open, start + STANDS), [pulls]);
    }

    #[test]
    fn a_listing_no_window_has_open_is_not_read() {
        let read = HashMap::new();
        let was = BTreeSet::from([listing("/r", Listing::Issues)]);
        assert!(due(&BTreeSet::new(), &read, &was, Instant::now()).is_empty());
    }

    #[test]
    fn a_repository_is_told_while_on_github_and_gh_runs_and_told_gone_once() {
        let now = Instant::now();
        let mut kept = Kept::default();
        let remote = github::Remote {
            name: "origin".into(),
            slug: "o/r".into(),
        };
        kept.remotes
            .insert("/r".into(), (Some(remote.clone()), now));
        kept.remotes.insert("/plain".into(), (None, now));
        kept.read.insert(
            listing("/r", Listing::Issues),
            (Err("gh is not logged in".into()), now),
        );
        let roots = BTreeSet::from(["/r".into(), "/plain".into()]);
        let told = kept.changes(true, &roots);
        assert_eq!(
            told,
            [TreeChange::GitHub {
                repository: "/r".into(),
                github: Some(Box::new(Repository {
                    slug: "o/r".into(),
                    pulls: None,
                    issues: Some(Err("gh is not logged in".into())),
                })),
            }]
        );
        let none = TreeChange::GitHub {
            repository: "/r".into(),
            github: None,
        };
        assert_eq!(
            kept.changes(false, &roots),
            vec![none.clone()],
            "gh is gone"
        );
        assert!(kept.changes(false, &roots).is_empty(), "said once");
        kept.changes(true, &roots);
        assert_eq!(kept.changes(true, &BTreeSet::new()), [none], "unlisted");
    }
}
