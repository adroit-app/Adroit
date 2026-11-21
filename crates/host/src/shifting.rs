//! Copying, moving and trashing the files and directories a window points
//! at in the tree's files.

use std::fs;
use std::io::ErrorKind;
use std::path::{Path, PathBuf};

/// Copy each of `sources` into `dir` by its own name, or move it there when
/// `moving`. One whose name is taken in `dir` already, or a directory put
/// into itself, is left where it is and said why; the rest are still done.
pub(crate) fn paste(sources: &[PathBuf], dir: &Path, moving: bool) -> Result<(), String> {
    let failed: Vec<String> = sources
        .iter()
        .filter_map(|source| pasted(source, dir, moving).err())
        .collect();
    if failed.is_empty() {
        Ok(())
    } else {
        Err(failed.join("; "))
    }
}

fn pasted(source: &Path, dir: &Path, moving: bool) -> Result<(), String> {
    let Some(name) = source.file_name() else {
        return Err(format!("{} has no name to paste by", source.display()));
    };
    let target = dir.join(name);
    if fs::symlink_metadata(source).is_err() {
        return Err(format!("{} is not there", source.display()));
    }
    if dir.starts_with(source) {
        return Err(format!("{} cannot be put inside itself", source.display()));
    }
    if fs::symlink_metadata(&target).is_ok() {
        return Err(format!("{} is there already", target.display()));
    }
    let done = if moving {
        moved(source, &target)
    } else {
        copied(source, &target)
    };
    done.map_err(|e| {
        let verb = if moving { "moved" } else { "copied" };
        format!("{} could not be {verb}: {e}", source.display())
    })
}

/// `source` renamed to `target`, or copied there and removed when they are
/// on different disks, which a rename cannot cross.
fn moved(source: &Path, target: &Path) -> std::io::Result<()> {
    match fs::rename(source, target) {
        Err(e) if e.kind() == ErrorKind::CrossesDevices => {
            copied(source, target)?;
            if fs::symlink_metadata(source)?.is_dir() {
                fs::remove_dir_all(source)
            } else {
                fs::remove_file(source)
            }
        }
        done => done,
    }
}

/// `source` copied to `target`, a directory with all it holds; a symbolic
/// link is copied as the link, not what it points at, so a link to a
/// directory above cannot make the copy endless.
fn copied(source: &Path, target: &Path) -> std::io::Result<()> {
    let meta = fs::symlink_metadata(source)?;
    if meta.file_type().is_symlink() {
        return linked(&fs::read_link(source)?, source, target);
    }
    if meta.is_dir() {
        fs::create_dir(target)?;
        for entry in fs::read_dir(source)? {
            let entry = entry?;
            copied(&entry.path(), &target.join(entry.file_name()))?;
        }
        return fs::set_permissions(target, meta.permissions());
    }
    fs::copy(source, target).map(drop)
}

#[cfg(unix)]
fn linked(to: &Path, _source: &Path, target: &Path) -> std::io::Result<()> {
    std::os::unix::fs::symlink(to, target)
}

#[cfg(windows)]
fn linked(to: &Path, source: &Path, target: &Path) -> std::io::Result<()> {
    if fs::metadata(source).is_ok_and(|m| m.is_dir()) {
        std::os::windows::fs::symlink_dir(to, target)
    } else {
        std::os::windows::fs::symlink_file(to, target)
    }
}

/// Put each of `paths` in the work machine's trash, where it can be had
/// back from. Those that cannot be are said why.
pub(crate) fn trash(paths: &[PathBuf]) -> Result<(), String> {
    let context = trash_context();
    context
        .delete_all(paths)
        .map_err(|e| format!("could not move to the trash: {e}"))
}

#[cfg(target_os = "macos")]
fn trash_context() -> trash::TrashContext {
    use trash::macos::{DeleteMethod, TrashContextExtMacos};
    let mut context = trash::TrashContext::default();
    // The Finder's way asks it through Apple Events, which a work host
    // started outside a desktop session, or not let to, cannot send.
    context.set_delete_method(DeleteMethod::NsFileManager);
    context
}

#[cfg(not(target_os = "macos"))]
fn trash_context() -> trash::TrashContext {
    trash::TrashContext::default()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tree() -> tempfile::TempDir {
        let root = tempfile::tempdir().unwrap();
        fs::create_dir_all(root.path().join("src/nested")).unwrap();
        fs::write(root.path().join("src/main.rs"), "fn main() {}").unwrap();
        fs::write(root.path().join("src/nested/lib.rs"), "lib").unwrap();
        fs::create_dir(root.path().join("dest")).unwrap();
        root
    }

    #[test]
    fn a_file_is_copied_into_the_directory_by_its_own_name() {
        let root = tree();
        let file = root.path().join("src/main.rs");
        paste(
            std::slice::from_ref(&file),
            &root.path().join("dest"),
            false,
        )
        .unwrap();
        assert_eq!(
            fs::read_to_string(root.path().join("dest/main.rs")).unwrap(),
            "fn main() {}"
        );
        assert!(file.exists());
    }

    #[test]
    fn a_directory_is_copied_with_all_it_holds() {
        let root = tree();
        paste(&[root.path().join("src")], &root.path().join("dest"), false).unwrap();
        assert_eq!(
            fs::read_to_string(root.path().join("dest/src/nested/lib.rs")).unwrap(),
            "lib"
        );
        assert!(root.path().join("src/nested/lib.rs").exists());
    }

    #[cfg(unix)]
    #[test]
    fn a_symbolic_link_is_copied_as_the_link() {
        let root = tree();
        let link = root.path().join("src/up");
        std::os::unix::fs::symlink("..", &link).unwrap();
        paste(&[root.path().join("src")], &root.path().join("dest"), false).unwrap();
        let copy = root.path().join("dest/src/up");
        assert_eq!(fs::read_link(copy).unwrap(), Path::new(".."));
    }

    #[test]
    fn moving_leaves_nothing_where_it_was() {
        let root = tree();
        let src = root.path().join("src");
        paste(std::slice::from_ref(&src), &root.path().join("dest"), true).unwrap();
        assert!(!src.exists());
        assert!(root.path().join("dest/src/main.rs").exists());
    }

    #[test]
    fn a_name_taken_already_is_left_as_it_is_and_the_rest_are_done() {
        let root = tree();
        let dest = root.path().join("dest");
        fs::write(dest.join("main.rs"), "kept").unwrap();
        for moving in [false, true] {
            let err = paste(&[root.path().join("src/main.rs")], &dest, moving).unwrap_err();
            assert!(err.contains("there already"), "{err}");
        }
        assert_eq!(fs::read_to_string(dest.join("main.rs")).unwrap(), "kept");
        assert!(root.path().join("src/main.rs").exists());

        let err = paste(
            &[
                root.path().join("src/main.rs"),
                root.path().join("src/nested"),
            ],
            &dest,
            false,
        )
        .unwrap_err();
        assert!(err.contains("main.rs"), "{err}");
        assert!(dest.join("nested/lib.rs").exists());
    }

    #[test]
    fn pasting_beside_itself_is_a_name_taken() {
        let root = tree();
        let err = paste(
            &[root.path().join("src/main.rs")],
            &root.path().join("src"),
            false,
        )
        .unwrap_err();
        assert!(err.contains("there already"), "{err}");
    }

    #[test]
    fn a_directory_is_never_put_inside_itself() {
        let root = tree();
        let src = root.path().join("src");
        for moving in [false, true] {
            let err = paste(std::slice::from_ref(&src), &src.join("nested"), moving).unwrap_err();
            assert!(err.contains("inside itself"), "{err}");
        }
        assert!(src.join("main.rs").exists());
        assert!(!src.join("nested/src").exists());
    }

    #[test]
    fn what_is_not_there_is_said_so() {
        let root = tree();
        let err = paste(
            &[root.path().join("gone.rs")],
            &root.path().join("dest"),
            false,
        )
        .unwrap_err();
        assert!(err.contains("not there"), "{err}");
    }
}
