//! A worktree's files as the tree lists them under its Files heading: the
//! entries of each directory opened, in the order they are listed.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

/// A file or a directory the tree lists.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Entry {
    pub path: PathBuf,
    pub dir: bool,
}

/// Directories the tree never lists: the version control's own.
const UNLISTED: &[&str] = &[".git", ".jj"];

/// `entries` with the version control's directories left out, directories
/// first and each kind by name, as fern lists them.
pub fn sorted(mut entries: Vec<Entry>) -> Vec<Entry> {
    entries.retain(|e| {
        !e.path
            .file_name()
            .is_some_and(|n| UNLISTED.contains(&n.to_string_lossy().as_ref()))
    });
    entries.sort_by_cached_key(|e| (!e.dir, name(&e.path).to_lowercase(), name(&e.path)));
    entries
}

fn name(path: &Path) -> String {
    path.file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default()
}

/// Whether `path` is hidden, by a leading dot.
pub fn is_hidden(path: &Path) -> bool {
    name(path).starts_with('.')
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(path: &str, dir: bool) -> Entry {
        Entry {
            path: path.into(),
            dir,
        }
    }

    #[test]
    fn directories_come_before_files_each_by_name_whatever_the_case() {
        let got = sorted(vec![
            entry("/w/b.rs", false),
            entry("/w/Src", true),
            entry("/w/A.md", false),
            entry("/w/docs", true),
        ]);
        assert_eq!(
            got,
            [
                entry("/w/docs", true),
                entry("/w/Src", true),
                entry("/w/A.md", false),
                entry("/w/b.rs", false),
            ]
        );
    }

    #[test]
    fn the_version_control_s_own_directories_are_never_listed() {
        let got = sorted(vec![
            entry("/w/.git", true),
            entry("/w/.jj", true),
            entry("/w/.github", true),
        ]);
        assert_eq!(got, [entry("/w/.github", true)]);
    }

    #[test]
    fn a_name_with_a_leading_dot_is_hidden() {
        assert!(is_hidden(Path::new("/w/.github")));
        assert!(!is_hidden(Path::new("/w/src")));
    }
}
