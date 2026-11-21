//! Copying, moving and trashing the files and directories a window points
//! at in the tree's files.

use std::fs;
use std::io::ErrorKind;
use std::path::{Path, PathBuf};

use adroit_protocol::PasteEntry;

/// What a paste did.
#[derive(Debug, Default, PartialEq, Eq)]
pub(crate) struct Pasted {
    /// The entries pasted.
    pub pasted: Vec<PasteEntry>,
    /// The entries whose target was there already, left as they are.
    pub taken: Vec<PasteEntry>,
    /// Why the rest could not be pasted.
    pub failed: Vec<String>,
}

/// Copy each entry's source to its target, or move it there when `moving`.
/// A target there already is left as it is and told as taken, unless the
/// entry says to replace it, when `discard` takes it away first; one
/// entry's failing does not stop the rest.
pub(crate) fn paste(
    entries: Vec<PasteEntry>,
    moving: bool,
    discard: impl Fn(&Path) -> Result<(), String>,
) -> Pasted {
    let mut done = Pasted::default();
    for entry in entries {
        match pasted(&entry, moving, &discard) {
            Ok(true) => done.pasted.push(entry),
            Ok(false) => done.taken.push(entry),
            Err(why) => done.failed.push(why),
        }
    }
    done
}

/// Whether `entry` was pasted, or else found its target taken.
fn pasted(
    entry: &PasteEntry,
    moving: bool,
    discard: impl Fn(&Path) -> Result<(), String>,
) -> Result<bool, String> {
    let PasteEntry {
        source,
        target,
        replace,
    } = entry;
    if fs::symlink_metadata(source).is_err() {
        return Err(format!("{} is not there", source.display()));
    }
    // Compared where they really are, so a link in the way cannot hide a
    // directory pasted into itself.
    let (real_source, real_target) = (real(source), real(target));
    if real_target != real_source && real_target.starts_with(&real_source) {
        return Err(format!("{} cannot be put inside itself", source.display()));
    }
    if fs::symlink_metadata(target).is_ok() {
        if !replace {
            return Ok(false);
        }
        if real_target == real_source {
            return Err(format!("{} cannot replace itself", source.display()));
        }
        if real_source.starts_with(&real_target) {
            return Err(format!(
                "{} holds {} and cannot be replaced by it",
                target.display(),
                source.display()
            ));
        }
        discard(target)?;
    }
    if let Some(parent) = target.parent() {
        fs::create_dir_all(parent)
            .map_err(|e| format!("{} could not be made: {e}", parent.display()))?;
    }
    let done = if moving {
        moved(source, target)
    } else {
        copied(source, target)
    };
    done.map(|()| true).map_err(|e| {
        let verb = if moving { "moved" } else { "copied" };
        format!("{} could not be {verb}: {e}", source.display())
    })
}

/// `path` with the directories it is in resolved to where they really are,
/// the nearest of them that exists at least; its own name is kept as it
/// is, as a link pasted is pasted as the link.
fn real(path: &Path) -> PathBuf {
    let mut names = Vec::new();
    let mut at = path;
    while let (Some(parent), Some(name)) = (at.parent(), at.file_name()) {
        names.push(name);
        if let Ok(resolved) = fs::canonicalize(parent) {
            return names.iter().rev().fold(resolved, |p, n| p.join(n));
        }
        at = parent;
    }
    path.to_path_buf()
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

    fn entry(source: PathBuf, target: PathBuf) -> PasteEntry {
        PasteEntry {
            source,
            target,
            replace: false,
        }
    }

    fn targets(done: &Pasted) -> Vec<PathBuf> {
        done.pasted.iter().map(|e| e.target.clone()).collect()
    }

    /// Takes a target away as the trash would, without the trash.
    fn removed(path: &Path) -> Result<(), String> {
        if path.is_dir() {
            fs::remove_dir_all(path)
        } else {
            fs::remove_file(path)
        }
        .map_err(|e| e.to_string())
    }

    fn never(path: &Path) -> Result<(), String> {
        panic!("{} discarded", path.display())
    }

    #[test]
    fn a_file_is_copied_to_its_target() {
        let root = tree();
        let file = root.path().join("src/main.rs");
        let target = root.path().join("dest/main.rs");
        let done = paste(vec![entry(file.clone(), target.clone())], false, never);
        assert_eq!(targets(&done), std::slice::from_ref(&target));
        assert_eq!(fs::read_to_string(target).unwrap(), "fn main() {}");
        assert!(file.exists());
    }

    #[test]
    fn a_directory_is_copied_with_all_it_holds() {
        let root = tree();
        let target = root.path().join("dest/src");
        paste(
            vec![entry(root.path().join("src"), target.clone())],
            false,
            never,
        );
        assert_eq!(
            fs::read_to_string(target.join("nested/lib.rs")).unwrap(),
            "lib"
        );
        assert!(root.path().join("src/nested/lib.rs").exists());
    }

    #[cfg(unix)]
    #[test]
    fn a_symbolic_link_is_copied_as_the_link() {
        let root = tree();
        std::os::unix::fs::symlink("..", root.path().join("src/up")).unwrap();
        let target = root.path().join("dest/src");
        paste(
            vec![entry(root.path().join("src"), target.clone())],
            false,
            never,
        );
        assert_eq!(fs::read_link(target.join("up")).unwrap(), Path::new(".."));
    }

    #[test]
    fn moving_leaves_nothing_where_it_was() {
        let root = tree();
        let src = root.path().join("src");
        paste(
            vec![entry(src.clone(), root.path().join("dest/src"))],
            true,
            never,
        );
        assert!(!src.exists());
        assert!(root.path().join("dest/src/main.rs").exists());
    }

    #[test]
    fn a_target_taken_is_left_as_it_is_and_told_while_the_rest_are_done() {
        let root = tree();
        let dest = root.path().join("dest");
        fs::write(dest.join("main.rs"), "kept").unwrap();
        for moving in [false, true] {
            let clash = entry(root.path().join("src/main.rs"), dest.join("main.rs"));
            let free = entry(
                root.path().join("src/nested"),
                dest.join(format!("n{moving}")),
            );
            let done = paste(vec![clash.clone(), free.clone()], moving, never);
            assert_eq!(done.taken, [clash]);
            assert_eq!(targets(&done), [free.target]);
            assert_eq!(done.failed, Vec::<String>::new());
        }
        assert_eq!(fs::read_to_string(dest.join("main.rs")).unwrap(), "kept");
        assert!(root.path().join("src/main.rs").exists());
    }

    #[test]
    fn a_target_replaced_is_discarded_before_the_source_takes_its_place() {
        let root = tree();
        let dest = root.path().join("dest");
        fs::create_dir(dest.join("main.rs")).unwrap();
        fs::write(dest.join("main.rs/old"), "old").unwrap();
        let replacing = PasteEntry {
            replace: true,
            ..entry(root.path().join("src/main.rs"), dest.join("main.rs"))
        };
        let done = paste(vec![replacing], false, removed);
        assert_eq!(targets(&done), [dest.join("main.rs")]);
        assert_eq!(
            fs::read_to_string(dest.join("main.rs")).unwrap(),
            "fn main() {}"
        );
    }

    #[test]
    fn a_target_that_cannot_be_discarded_is_kept_and_said_why() {
        let root = tree();
        let dest = root.path().join("dest");
        fs::write(dest.join("main.rs"), "kept").unwrap();
        let replacing = PasteEntry {
            replace: true,
            ..entry(root.path().join("src/main.rs"), dest.join("main.rs"))
        };
        let done = paste(vec![replacing], false, |_| Err("no trash".into()));
        assert_eq!(done.failed, ["no trash"]);
        assert_eq!(fs::read_to_string(dest.join("main.rs")).unwrap(), "kept");
    }

    #[test]
    fn pasting_beside_itself_is_a_target_taken_and_never_replaces_itself() {
        let root = tree();
        let file = root.path().join("src/main.rs");
        let done = paste(vec![entry(file.clone(), file.clone())], false, never);
        assert_eq!(done.taken.len(), 1);
        let replacing = PasteEntry {
            replace: true,
            ..entry(file.clone(), file.clone())
        };
        let done = paste(vec![replacing], false, never);
        assert!(done.failed[0].contains("itself"), "{:?}", done.failed);
        assert!(file.exists());
    }

    #[test]
    fn a_renamed_target_makes_the_directories_it_passes_through() {
        let root = tree();
        let target = root.path().join("dest/a/b/copy.rs");
        let done = paste(
            vec![entry(root.path().join("src/main.rs"), target.clone())],
            false,
            never,
        );
        assert_eq!(targets(&done), std::slice::from_ref(&target));
        assert!(target.exists());
    }

    #[test]
    fn a_directory_is_never_put_inside_itself() {
        let root = tree();
        let src = root.path().join("src");
        for moving in [false, true] {
            let done = paste(
                vec![entry(src.clone(), src.join("nested/src"))],
                moving,
                never,
            );
            assert!(
                done.failed[0].contains("inside itself"),
                "{:?}",
                done.failed
            );
        }
        assert!(src.join("main.rs").exists());
        assert!(!src.join("nested/src").exists());
    }

    #[test]
    fn a_directory_holding_the_source_is_never_replaced_by_it() {
        let root = tree();
        let source = root.path().join("src/nested");
        let replacing = PasteEntry {
            replace: true,
            ..entry(source.clone(), root.path().join("src"))
        };
        for moving in [false, true] {
            let done = paste(vec![replacing.clone()], moving, never);
            assert!(done.failed[0].contains("holds"), "{:?}", done.failed);
        }
        assert!(source.join("lib.rs").exists());
    }

    #[cfg(unix)]
    #[test]
    fn a_directory_is_never_put_inside_itself_through_a_link() {
        let root = tree();
        let src = root.path().join("src");
        std::os::unix::fs::symlink(src.join("nested"), root.path().join("link")).unwrap();
        let done = paste(
            vec![entry(src.clone(), root.path().join("link/src"))],
            false,
            never,
        );
        assert!(
            done.failed[0].contains("inside itself"),
            "{:?}",
            done.failed
        );
        assert!(!src.join("nested/src").exists());
    }

    #[test]
    fn what_is_not_there_is_said_so() {
        let root = tree();
        let done = paste(
            vec![entry(
                root.path().join("gone.rs"),
                root.path().join("dest/gone.rs"),
            )],
            false,
            never,
        );
        assert!(done.failed[0].contains("not there"), "{:?}", done.failed);
    }
}
