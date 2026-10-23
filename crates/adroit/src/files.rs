//! A worktree's files as the tree lists them under its Files heading: each
//! directory's entries as it is opened, and what jj or git says of each.

use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};

use adroit_claude::git::Change;

/// A file or a directory the tree lists.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Entry {
    pub path: PathBuf,
    pub dir: bool,
}

/// Directories the tree never lists: the version control's own.
const UNLISTED: &[&str] = &[".git", ".jj"];

/// The entries directly inside `dir`, as [`sorted`] orders them. A link to
/// a directory is a directory; what cannot be read is left out.
pub fn read(dir: &Path) -> Vec<Entry> {
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

    fn entry(path: &str, dir: bool) -> Entry {
        Entry {
            path: path.into(),
            dir,
        }
    }

    fn change(status: &str, path: &str) -> Change {
        Change {
            status: status.into(),
            path: path.into(),
            from: None,
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

    #[test]
    fn a_name_with_a_leading_dot_is_hidden() {
        assert!(is_hidden(Path::new("/w/.github")));
        assert!(!is_hidden(Path::new("/w/src")));
    }
}
