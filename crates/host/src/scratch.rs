//! Scratch sessions: every one runs in the scratch folder, made when the
//! first one starts.

use std::io;
use std::path::{Path, PathBuf};

/// Where a session started from a draft in `worktree` runs: the scratch
/// folder `root`, made if it is not there yet, for a draft in it, otherwise
/// the draft's own directory.
pub fn start_dir(worktree: &Path, root: &Path) -> io::Result<PathBuf> {
    if worktree == root {
        std::fs::create_dir_all(root)?;
    }
    Ok(worktree.to_path_buf())
}

/// Whether a session working in `dir` is a scratch session: one in the
/// scratch folder `root` or below it.
pub fn holds(root: &Path, dir: &Path) -> bool {
    dir.starts_with(root)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_draft_in_the_scratch_folder_starts_in_the_folder_itself_made_with_it() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path().join("adroit").join("scratch");
        let first = start_dir(&root, &root).unwrap();
        let second = start_dir(&root, &root).unwrap();
        assert_eq!(first, root);
        assert_eq!(second, root);
        assert!(root.is_dir());
        assert_eq!(std::fs::read_dir(&root).unwrap().count(), 0);
    }

    #[test]
    fn a_draft_anywhere_else_starts_where_it_is_and_makes_nothing() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path().join("scratch");
        let inside = root.join("20260309-0705");
        assert_eq!(start_dir(&inside, &root).unwrap(), inside);
        assert_eq!(
            start_dir(Path::new("/w/api"), &root).unwrap(),
            PathBuf::from("/w/api")
        );
        assert!(!root.exists());
    }

    #[test]
    fn a_session_in_the_scratch_folder_or_below_it_is_a_scratch_session() {
        let root = Path::new("/d/adroit/scratch");
        assert!(holds(root, root));
        assert!(holds(root, &root.join("20260309-0705")));
        assert!(!holds(root, Path::new("/d/adroit/scratchpad")));
        assert!(!holds(root, Path::new("/w/api")));
    }
}
