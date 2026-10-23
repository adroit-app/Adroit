//! What jj or git says of each file of a worktree, as the tree marks it.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

use crate::Change;

/// What jj or git says of the files of one worktree.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Marks {
    changed: HashMap<PathBuf, char>,
    ignored: HashSet<PathBuf>,
}

/// The mark of a directory holding a changed file.
pub const HOLDS_CHANGES: char = '•';

impl Marks {
    /// The marks of the worktree at `root` from what git lists: `changes`
    /// and the `ignored` paths, both relative to it. In a jj workspace a
    /// file git does not track is one jj added.
    pub fn new(root: &Path, changes: &[Change], ignored: &[String], jj: bool) -> Self {
        Marks {
            changed: changes
                .iter()
                .map(|c| (root.join(&c.path), letter(&c.status, jj)))
                .collect(),
            ignored: ignored
                .iter()
                .map(|p| root.join(p.trim_end_matches('/')))
                .collect(),
        }
    }

    /// The letter of a changed file, or [`HOLDS_CHANGES`] for a directory
    /// holding one.
    pub fn of(&self, path: &Path, dir: bool) -> Option<char> {
        if dir {
            return self
                .changed
                .keys()
                .any(|p| p.starts_with(path) && p != path)
                .then_some(HOLDS_CHANGES);
        }
        self.changed.get(path).copied()
    }

    /// Whether git ignores `path`, itself or by a directory above it.
    pub fn ignores(&self, path: &Path) -> bool {
        path.ancestors().any(|p| self.ignored.contains(p))
    }
}

/// The letter for git's two-letter status: the index's, else the
/// worktree's.
fn letter(status: &str, jj: bool) -> char {
    match status {
        "??" if jj => 'A',
        "??" => '?',
        _ => status.trim().chars().next().unwrap_or('M'),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn change(status: &str, path: &str) -> Change {
        Change {
            status: status.into(),
            path: path.into(),
            from: None,
        }
    }

    #[test]
    fn a_changed_file_is_marked_by_git_s_letter_for_it() {
        let marks = Marks::new(
            Path::new("/w"),
            &[
                change(" M", "src/lib.rs"),
                change("A ", "new.rs"),
                change("R ", "moved.rs"),
            ],
            &[],
            false,
        );
        assert_eq!(marks.of(Path::new("/w/src/lib.rs"), false), Some('M'));
        assert_eq!(marks.of(Path::new("/w/new.rs"), false), Some('A'));
        assert_eq!(marks.of(Path::new("/w/moved.rs"), false), Some('R'));
        assert_eq!(marks.of(Path::new("/w/kept.rs"), false), None);
    }

    #[test]
    fn a_file_git_does_not_track_is_added_in_jj_and_untracked_in_git() {
        let untracked = [change("??", "new.rs")];
        let jj = Marks::new(Path::new("/w"), &untracked, &[], true);
        let git = Marks::new(Path::new("/w"), &untracked, &[], false);
        assert_eq!(jj.of(Path::new("/w/new.rs"), false), Some('A'));
        assert_eq!(git.of(Path::new("/w/new.rs"), false), Some('?'));
    }

    #[test]
    fn a_directory_holding_a_changed_file_is_marked_at_any_depth() {
        let marks = Marks::new(Path::new("/w"), &[change(" M", "src/a/b.rs")], &[], false);
        assert_eq!(marks.of(Path::new("/w/src"), true), Some(HOLDS_CHANGES));
        assert_eq!(marks.of(Path::new("/w/src/a"), true), Some(HOLDS_CHANGES));
        assert_eq!(marks.of(Path::new("/w/docs"), true), None);
        // A file whose name starts as the directory's does is not in it.
        assert_eq!(marks.of(Path::new("/w/sr"), true), None);
    }

    #[test]
    fn what_lies_in_an_ignored_directory_is_ignored_too() {
        let marks = Marks::new(
            Path::new("/w"),
            &[],
            &["target/".into(), "run.log".into()],
            false,
        );
        assert!(marks.ignores(Path::new("/w/target")));
        assert!(marks.ignores(Path::new("/w/target/debug/a")));
        assert!(marks.ignores(Path::new("/w/run.log")));
        assert!(!marks.ignores(Path::new("/w/src")));
    }
}
