//! Making the files and directories a window names in the tree's files.

use std::fs::{self, OpenOptions};
use std::io::ErrorKind;
use std::path::Path;

/// Make `path`, a directory when `dir` or else an empty file, with the
/// directories it passes through. One already there is left as it is.
pub(crate) fn make(path: &Path, dir: bool) -> Result<(), String> {
    // Windows refuses to open a directory as a new file with Access is
    // denied, not AlreadyExists, so what stands at the path is asked too.
    let failed = |e: std::io::Error| {
        if e.kind() == ErrorKind::AlreadyExists || fs::symlink_metadata(path).is_ok() {
            format!("{} is there already", path.display())
        } else {
            format!("{} could not be made: {e}", path.display())
        }
    };
    if dir {
        return fs::create_dir(path)
            .or_else(|e| match e.kind() {
                ErrorKind::NotFound => fs::create_dir_all(path),
                _ => Err(e),
            })
            .map_err(failed);
    }
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(failed)?;
    }
    OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
        .map(drop)
        .map_err(failed)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_file_is_made_empty_with_the_directories_it_passes_through() {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("a/b/c.rs");
        make(&path, false).unwrap();
        assert_eq!(fs::read(&path).unwrap(), b"");
    }

    #[test]
    fn a_directory_is_made_with_the_directories_it_passes_through() {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("a/b");
        make(&path, true).unwrap();
        assert!(path.is_dir());
    }

    #[test]
    fn what_is_there_already_is_left_as_it_is() {
        let root = tempfile::tempdir().unwrap();
        let file = root.path().join("kept.txt");
        fs::write(&file, "kept").unwrap();
        let dir = root.path().join("src");
        fs::create_dir(&dir).unwrap();
        for (path, as_dir) in [(&file, false), (&file, true), (&dir, false), (&dir, true)] {
            let err = make(path, as_dir).unwrap_err();
            assert!(err.contains("there already"), "{err}");
        }
        assert_eq!(fs::read_to_string(&file).unwrap(), "kept");
    }
}
