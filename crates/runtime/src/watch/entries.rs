//! What each directory the tree or the picker lists holds, read again every two seconds
//! so files made or removed meanwhile show.

use std::fs;
use std::path::Path;
use std::time::Duration;

use adroit_model::files::{Entry, sorted};
use adroit_protocol::TreeChange;

use super::Watched;

pub(super) async fn watch(mut watched: Watched) {
    loop {
        let dirs = watched.sight().listed_dirs();
        let read = tokio::task::spawn_blocking(move || {
            dirs.into_iter()
                .map(|dir| {
                    let entries = read(&dir);
                    TreeChange::Entries { dir, entries }
                })
                .collect::<Vec<_>>()
        })
        .await;
        if let Ok(read) = read {
            watched.record(read);
        }
        watched
            .wait(Duration::from_secs(2), |s| s.sight.listed_dirs())
            .await;
    }
}

/// The entries directly inside `dir`, as [`sorted`] orders them. A link to
/// a directory is a directory; what cannot be read is left out.
fn read(dir: &Path) -> Vec<Entry> {
    let Ok(entries) = fs::read_dir(dir) else {
        return Vec::new();
    };
    sorted(
        entries
            .filter_map(Result::ok)
            .map(|e| {
                let path = e.path();
                let dir = path.is_dir();
                Entry { path, dir }
            })
            .collect(),
    )
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::*;

    fn dirs(dir: &Path) -> Vec<PathBuf> {
        let mut found: Vec<PathBuf> = read(dir)
            .into_iter()
            .filter(|e| e.dir)
            .map(|e| e.path)
            .collect();
        found.sort();
        found
    }

    #[test]
    fn reading_a_directory_answers_its_subdirectories_and_links_to_them_as_directories() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        fs::create_dir(root.join("dir")).unwrap();
        fs::create_dir(root.join(".hidden")).unwrap();
        fs::write(root.join("file"), "").unwrap();
        std::os::unix::fs::symlink(root.join("dir"), root.join("to-dir")).unwrap();
        std::os::unix::fs::symlink(root.join("file"), root.join("to-file")).unwrap();
        std::os::unix::fs::symlink(root.join("gone"), root.join("dangling")).unwrap();

        assert_eq!(
            dirs(root),
            vec![root.join(".hidden"), root.join("dir"), root.join("to-dir")]
        );
    }

    #[test]
    fn a_directory_that_cannot_be_read_has_no_entries() {
        assert!(read(Path::new("/no/such/directory")).is_empty());
    }
}
