//! The workspaces the tree keeps, kept in the instance's state folder from
//! one run to the next: each one something ran in, or one the user added,
//! until the user takes it out of the tree.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

const FILE: &str = "workspaces.json";

/// Taken while what is kept is read, changed and written again.
static KEEPING: Mutex<()> = Mutex::new(());

/// The workspaces kept in `directory`, none before any is.
pub fn load(directory: &Path) -> Result<BTreeSet<PathBuf>, String> {
    let path = directory.join(FILE);
    match std::fs::read(&path) {
        Ok(bytes) => serde_json::from_slice(&bytes)
            .map_err(|error| format!("cannot read {}: {error}", path.display())),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(BTreeSet::new()),
        Err(error) => Err(format!("cannot read {}: {error}", path.display())),
    }
}

/// Keep `roots` in `directory` besides what it keeps; whether that kept any
/// not kept before. What cannot be read is left as it is rather than written
/// over.
pub fn keep(directory: &Path, roots: impl IntoIterator<Item = PathBuf>) -> Result<bool, String> {
    change(directory, |kept| {
        roots
            .into_iter()
            .fold(false, |added, root| kept.insert(root) | added)
    })
}

/// Keep `root` in `directory` no more; whether it was kept.
pub fn forget(directory: &Path, root: &Path) -> Result<bool, String> {
    change(directory, |kept| kept.remove(root))
}

fn change(
    directory: &Path,
    make: impl FnOnce(&mut BTreeSet<PathBuf>) -> bool,
) -> Result<bool, String> {
    let _keeping = KEEPING
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let mut kept = load(directory)?;
    if !make(&mut kept) {
        return Ok(false);
    }
    write(directory, &kept)?;
    Ok(true)
}

/// Written whole beside the file and moved over it, so a reader never sees
/// half of it.
fn write(directory: &Path, kept: &BTreeSet<PathBuf>) -> Result<(), String> {
    let temporary = directory.join(format!(".{FILE}-{}.tmp", std::process::id()));
    let result = (|| {
        std::fs::create_dir_all(directory).map_err(|error| error.to_string())?;
        let bytes = serde_json::to_vec_pretty(kept).map_err(|error| error.to_string())?;
        std::fs::write(&temporary, bytes).map_err(|error| error.to_string())?;
        std::fs::rename(&temporary, directory.join(FILE)).map_err(|error| error.to_string())
    })();
    if result.is_err() {
        let _ = std::fs::remove_file(&temporary);
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    fn kept(directory: &Path) -> Vec<PathBuf> {
        load(directory).unwrap().into_iter().collect()
    }

    #[test]
    fn nothing_is_kept_before_anything_is() {
        let directory = tempfile::tempdir().unwrap();
        assert!(kept(directory.path()).is_empty());
    }

    #[test]
    fn what_is_kept_stays_until_it_is_forgotten() {
        let directory = tempfile::tempdir().unwrap();
        let root = directory.path();
        assert!(keep(root, ["/api".into(), "/notes".into()]).unwrap());
        assert!(!keep(root, ["/api".into()]).unwrap());
        assert_eq!(kept(root), [PathBuf::from("/api"), "/notes".into()]);
        assert!(forget(root, Path::new("/api")).unwrap());
        assert!(!forget(root, Path::new("/api")).unwrap());
        assert_eq!(kept(root), [PathBuf::from("/notes")]);
    }

    #[test]
    fn a_file_that_cannot_be_read_is_not_written_over() {
        let directory = tempfile::tempdir().unwrap();
        let root = directory.path();
        std::fs::write(root.join(FILE), "not json").unwrap();
        assert!(keep(root, ["/api".into()]).is_err());
        assert_eq!(
            std::fs::read_to_string(root.join(FILE)).unwrap(),
            "not json"
        );
    }
}
