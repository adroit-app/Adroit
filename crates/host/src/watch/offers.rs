//! The directories the picker offers to start a session in, listed each
//! time a window opens the picker or asks for everything read again.

use std::path::PathBuf;
use std::time::Duration;

use adroit_agents::claude::places;
use adroit_protocol::{Offers, PickChange};
use adroit_vcs::ghq;

use super::Watched;
use crate::work::Seen;

pub(super) async fn watch(mut watched: Watched) {
    loop {
        if picking(&watched.seen()) {
            watched.record([PickChange::Offers {
                offers: read().await,
            }]);
        }
        watched.wait(Duration::from_secs(300), picking).await;
    }
}

/// Whether a window shows the picker.
fn picking(seen: &[Seen]) -> bool {
    seen.iter().any(|s| s.sight.pick.is_some())
}

/// Where an agent was used, ghq's repositories and roots, and the folders
/// in the roots the settings list.
async fn read() -> Offers {
    let home = crate::paths::home();
    let asked = crate::settings::current().picker;
    let listed_roots = asked.root_paths(&home);
    // Claude Code's state file can run to megabytes, and every directory it
    // names is looked at: not on a runtime thread.
    let (recent, (ghq, ghq_roots), listed) = tokio::join!(
        tokio::task::spawn_blocking(move || {
            // Every connection Claude Code runs in keeps its own.
            let mut recent: Vec<PathBuf> = Vec::new();
            let connections = crate::programs::connections(&crate::settings::current());
            for context in connections
                .iter()
                .filter(|c| c.agent() == adroit_model::Agent::Claude)
            {
                for dir in places::recent(context, &home) {
                    if !recent.contains(&dir) {
                        recent.push(dir);
                    }
                }
            }
            recent
        }),
        async {
            if asked.ghq {
                tokio::join!(ghq::list(), ghq::roots())
            } else {
                (Vec::new(), Vec::new())
            }
        },
        tokio::task::spawn_blocking({
            let roots = listed_roots.clone();
            move || folders_in(&roots)
        }),
    );
    Offers {
        recent: recent.unwrap_or_default(),
        ghq,
        listed: listed.unwrap_or_default(),
        roots: ghq_roots.into_iter().chain(listed_roots).collect(),
    }
}

/// The folders in `roots`, each root's in name order, leaving out hidden ones
/// and a root that cannot be read.
fn folders_in(roots: &[PathBuf]) -> Vec<PathBuf> {
    roots
        .iter()
        .flat_map(|root| {
            let mut found: Vec<PathBuf> = std::fs::read_dir(root)
                .into_iter()
                .flatten()
                .flatten()
                .filter(|e| !e.file_name().to_string_lossy().starts_with('.'))
                .filter(|e| e.path().is_dir())
                .map(|e| e.path())
                .collect();
            found.sort();
            found
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_root_lists_the_folders_in_it_but_hidden_ones_and_files() {
        let root = tempfile::tempdir().unwrap();
        for d in ["b", "a", ".git"] {
            std::fs::create_dir(root.path().join(d)).unwrap();
        }
        std::fs::write(root.path().join("file"), "").unwrap();
        let got = folders_in(&[root.path().to_path_buf(), PathBuf::from("/no/such")]);
        assert_eq!(got, [root.path().join("a"), root.path().join("b")]);
    }
}
