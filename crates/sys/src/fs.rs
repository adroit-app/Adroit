//! Modes, owners and open flags of files, as `std::os::unix::fs` and libc
//! give them on Unix.

#[cfg(unix)]
pub use libc::{ELOOP, O_NOFOLLOW, O_NONBLOCK};
#[cfg(unix)]
pub use std::os::unix::fs::{DirBuilderExt, MetadataExt, OpenOptionsExt, PermissionsExt};

/// Set the mode of the file at `path`.
#[cfg(unix)]
pub fn set_mode(path: impl AsRef<std::path::Path>, mode: u32) -> std::io::Result<()> {
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(mode))
}

/// No flag: a link is followed, as Windows opens one only when asked.
#[cfg(windows)]
pub const O_NOFOLLOW: i32 = 0;
/// No flag: Windows opens a file without waiting on it.
#[cfg(windows)]
pub const O_NONBLOCK: i32 = 0;
/// An error no open returns, as no open is refused for following a link.
#[cfg(windows)]
pub const ELOOP: i32 = i32::MIN;

/// Leave the file at `path` as it is: its profile's access list is its mode.
#[cfg(windows)]
pub fn set_mode(path: impl AsRef<std::path::Path>, _: u32) -> std::io::Result<()> {
    std::fs::metadata(path).map(drop)
}

#[cfg(windows)]
pub trait DirBuilderExt {
    fn mode(&mut self, mode: u32) -> &mut Self;
}

#[cfg(windows)]
impl DirBuilderExt for std::fs::DirBuilder {
    fn mode(&mut self, _: u32) -> &mut Self {
        self
    }
}

#[cfg(windows)]
pub trait OpenOptionsExt {
    fn mode(&mut self, mode: u32) -> &mut Self;
    fn custom_flags(&mut self, flags: i32) -> &mut Self;
}

#[cfg(windows)]
impl OpenOptionsExt for std::fs::OpenOptions {
    fn mode(&mut self, _: u32) -> &mut Self {
        self
    }
    fn custom_flags(&mut self, _: i32) -> &mut Self {
        self
    }
}

#[cfg(windows)]
pub trait MetadataExt {
    /// [`crate::uid`], the user every file is reported as owned by.
    fn uid(&self) -> u32;
    /// Open to the user alone: `0o700` for a folder, `0o600` for a file.
    fn mode(&self) -> u32;
    /// One name: Windows hard links are not told apart here.
    fn nlink(&self) -> u64;
}

#[cfg(windows)]
impl MetadataExt for std::fs::Metadata {
    fn uid(&self) -> u32 {
        crate::uid()
    }
    fn mode(&self) -> u32 {
        if self.is_dir() { 0o040_700 } else { 0o100_600 }
    }
    fn nlink(&self) -> u64 {
        1
    }
}

#[cfg(windows)]
pub trait PermissionsExt {
    /// Open to the user alone, and runnable, as a file the user may run is.
    fn mode(&self) -> u32;
}

#[cfg(windows)]
impl PermissionsExt for std::fs::Permissions {
    fn mode(&self) -> u32 {
        0o700
    }
}

/// What tells the open `file` from one put in its place: its inode.
#[cfg(unix)]
pub fn file_id(file: &std::fs::File) -> std::io::Result<u64> {
    Ok(file.metadata()?.ino())
}

/// What tells the open `file` from one put in its place: its index on its
/// volume. Not its creation time, which a file renamed to a name just freed
/// takes over from the file that had it.
#[cfg(windows)]
pub fn file_id(file: &std::fs::File) -> std::io::Result<u64> {
    use std::os::windows::io::AsRawHandle;
    use windows_sys::Win32::Storage::FileSystem;
    // SAFETY: the handle is the open file's own, and the structure is the
    // one the call fills.
    let info = unsafe {
        let mut info = std::mem::zeroed::<FileSystem::BY_HANDLE_FILE_INFORMATION>();
        if FileSystem::GetFileInformationByHandle(file.as_raw_handle(), &mut info) == 0 {
            return Err(std::io::Error::last_os_error());
        }
        info
    };
    Ok(u64::from(info.nFileIndexHigh) << 32 | u64::from(info.nFileIndexLow))
}

/// A file standing for the folder `dir`, to lock: the folder itself.
#[cfg(unix)]
pub fn lockable(dir: impl AsRef<std::path::Path>) -> std::io::Result<std::fs::File> {
    std::fs::File::open(dir)
}

/// A file standing for the folder `dir`, to lock: one kept in it, as Windows
/// locks no folder.
#[cfg(windows)]
pub fn lockable(dir: impl AsRef<std::path::Path>) -> std::io::Result<std::fs::File> {
    std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(dir.as_ref().join(".lock"))
}

/// Write what the folder `dir` lists to the disk, so a file just moved or
/// made in it survives a crash.
#[cfg(unix)]
pub fn sync_dir(dir: impl AsRef<std::path::Path>) -> std::io::Result<()> {
    std::fs::File::open(dir)?.sync_all()
}

/// Nothing to write: Windows keeps a folder's listing with the file it names.
#[cfg(windows)]
pub fn sync_dir(_: impl AsRef<std::path::Path>) -> std::io::Result<()> {
    Ok(())
}

/// The absolute path `path` names, with every link resolved.
#[cfg(unix)]
pub fn canonicalize(path: impl AsRef<std::path::Path>) -> std::io::Result<std::path::PathBuf> {
    std::fs::canonicalize(path)
}

/// The absolute path `path` names, with every link resolved, written as
/// people and programs write it: Windows gives it as a verbatim path, which
/// a shell shows as it is and many programs refuse.
#[cfg(windows)]
pub fn canonicalize(path: impl AsRef<std::path::Path>) -> std::io::Result<std::path::PathBuf> {
    let path = std::fs::canonicalize(path)?;
    let text = path.to_string_lossy();
    Ok(if let Some(share) = text.strip_prefix(r"\\?\UNC\") {
        std::path::PathBuf::from(format!(r"\\{share}"))
    } else if let Some(local) = text
        .strip_prefix(r"\\?\")
        .filter(|rest| rest.as_bytes().get(1) == Some(&b':'))
    {
        std::path::PathBuf::from(local)
    } else {
        path
    })
}

/// Put `bytes` at `path` whole, in `mode`, so a reader sees the old file or
/// the new one and never half of either, and a crash leaves one of them.
/// What stood at `path`, a link included, is replaced rather than written
/// through. When it fails, what was there is left and nothing beside it.
pub fn replace(
    path: impl AsRef<std::path::Path>,
    bytes: impl AsRef<[u8]>,
    mode: u32,
) -> std::io::Result<()> {
    let path = path.as_ref();
    let staged = staged(path, bytes.as_ref(), mode)?;
    staged.persist(path).map_err(|error| error.error)?;
    sync_dir(parent(path))
}

/// Put `bytes` at `path` whole, in `mode`, as [`replace`] does, unless a file
/// is there already, which is kept: that fails as
/// [`std::io::ErrorKind::AlreadyExists`].
pub fn create_whole(
    path: impl AsRef<std::path::Path>,
    bytes: impl AsRef<[u8]>,
    mode: u32,
) -> std::io::Result<()> {
    let path = path.as_ref();
    let staged = staged(path, bytes.as_ref(), mode)?;
    std::fs::hard_link(staged.path(), path)?;
    drop(staged);
    sync_dir(parent(path))
}

fn parent(path: &std::path::Path) -> &std::path::Path {
    match path.parent() {
        Some(parent) if !parent.as_os_str().is_empty() => parent,
        _ => std::path::Path::new("."),
    }
}

/// `bytes` on the disk in a file of a name of its own beside `path`, which
/// is taken away when it is dropped unmoved.
fn staged(
    path: &std::path::Path,
    bytes: &[u8],
    mode: u32,
) -> std::io::Result<tempfile::NamedTempFile> {
    use std::io::Write;
    let prefix = format!(
        ".{}.",
        path.file_name().unwrap_or_default().to_string_lossy()
    );
    let mut builder = tempfile::Builder::new();
    builder.prefix(&prefix).suffix(".tmp");
    #[cfg(unix)]
    builder.permissions(std::fs::Permissions::from_mode(mode));
    #[cfg(windows)]
    let _ = mode;
    let mut staged = builder.tempfile_in(parent(path))?;
    staged.write_all(bytes)?;
    staged.as_file().sync_all()?;
    Ok(staged)
}

/// Make `path` a folder open to its user alone, with its missing parents,
/// unless one is there; one already there has to be a folder, not a link,
/// owned by this user and closed to everyone else.
pub fn private_dir(path: impl AsRef<std::path::Path>) -> std::io::Result<()> {
    let path = path.as_ref();
    match std::fs::DirBuilder::new().mode(0o700).create(path) {
        // Made in `0o700` less the umask, which may take the user's own bits.
        Ok(()) => set_mode(path, 0o700)?,
        Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {}
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            std::fs::DirBuilder::new()
                .recursive(true)
                .mode(0o700)
                .create(path)?;
            set_mode(path, 0o700)?;
        }
        Err(error) => return Err(error),
    }
    let metadata = std::fs::symlink_metadata(path)?;
    if !metadata.is_dir() || metadata.uid() != crate::euid() || metadata.mode() & 0o077 != 0 {
        return Err(std::io::Error::new(
            std::io::ErrorKind::PermissionDenied,
            format!("{} is not a private folder of this user", path.display()),
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_canonical_path_is_absolute_resolved_and_not_verbatim() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir(dir.path().join("a")).unwrap();
        let path = canonicalize(dir.path().join("a").join("..").join("a")).unwrap();
        assert!(path.is_absolute(), "{path:?}");
        assert!(!path.to_string_lossy().starts_with(r"\\?\"), "{path:?}");
        assert_eq!(path, canonicalize(dir.path()).unwrap().join("a"));
        assert!(path.is_dir());
    }

    #[test]
    fn a_folder_can_be_locked_by_one_holder_at_a_time() {
        let dir = tempfile::tempdir().unwrap();
        let first = lockable(dir.path()).unwrap();
        first.try_lock().unwrap();
        assert!(lockable(dir.path()).unwrap().try_lock().is_err());
        drop(first);
        // A process another test forks holds the lock with it until it runs
        // its program, so the lock may outlive its holder for a moment.
        let next = lockable(dir.path()).unwrap();
        let taken = (0..100).any(|_| {
            let taken = next.try_lock().is_ok();
            if !taken {
                std::thread::sleep(std::time::Duration::from_millis(10));
            }
            taken
        });
        assert!(taken);
    }

    #[test]
    fn a_file_put_in_another_s_place_at_once_has_another_id() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("f");
        std::fs::write(&path, "first").unwrap();
        let first = file_id(&std::fs::File::open(&path).unwrap()).unwrap();
        std::fs::write(&path, "rewritten").unwrap();
        assert_eq!(
            file_id(&std::fs::File::open(&path).unwrap()).unwrap(),
            first
        );
        let other = dir.path().join("other");
        std::fs::write(&other, "second").unwrap();
        std::fs::remove_file(&path).unwrap();
        std::fs::rename(&other, &path).unwrap();
        assert_ne!(
            file_id(&std::fs::File::open(&path).unwrap()).unwrap(),
            first
        );
    }

    #[test]
    fn a_file_made_private_reads_as_closed_to_others() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("f");
        std::fs::write(&file, "").unwrap();
        set_mode(&file, 0o600).unwrap();
        let mode = std::fs::metadata(&file).unwrap().mode() & 0o777;
        assert_eq!(mode, 0o600);
        sync_dir(dir.path()).unwrap();
    }

    fn listed(dir: &std::path::Path) -> Vec<String> {
        let mut names: Vec<String> = std::fs::read_dir(dir)
            .unwrap()
            .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
            .collect();
        names.sort();
        names
    }

    #[test]
    fn a_replaced_file_holds_the_new_bytes_in_its_mode_and_nothing_is_left_beside_it() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("settings.toml");
        std::fs::write(&path, "old").unwrap();

        replace(&path, b"new", 0o644).unwrap();

        assert_eq!(std::fs::read(&path).unwrap(), b"new");
        #[cfg(unix)]
        assert_eq!(std::fs::metadata(&path).unwrap().mode() & 0o777, 0o644);
        assert_eq!(listed(dir.path()), ["settings.toml"]);
        replace(dir.path().join("key"), b"secret", 0o600).unwrap();
        assert_eq!(
            std::fs::metadata(dir.path().join("key")).unwrap().mode() & 0o777,
            0o600
        );
    }

    #[test]
    fn a_replace_that_fails_leaves_what_was_there_and_no_file_beside_it() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("taken");
        std::fs::create_dir(&path).unwrap();
        std::fs::write(path.join("inside"), "kept").unwrap();

        assert!(replace(&path, b"new", 0o600).is_err());

        assert_eq!(listed(dir.path()), ["taken"]);
        assert_eq!(std::fs::read(path.join("inside")).unwrap(), b"kept");
    }

    #[cfg(unix)]
    #[test]
    fn a_replace_puts_a_file_in_place_of_a_link_rather_than_writing_through_it() {
        let dir = tempfile::tempdir().unwrap();
        let target = dir.path().join("target");
        std::fs::write(&target, "elsewhere").unwrap();
        let path = dir.path().join("link");
        std::os::unix::fs::symlink(&target, &path).unwrap();

        replace(&path, b"new", 0o600).unwrap();

        assert_eq!(std::fs::read(&target).unwrap(), b"elsewhere");
        assert!(std::fs::symlink_metadata(&path).unwrap().is_file());
    }

    #[test]
    fn a_file_made_whole_once_refuses_one_already_there_and_keeps_it() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("key");

        create_whole(&path, b"first", 0o600).unwrap();
        let refused = create_whole(&path, b"second", 0o600).unwrap_err();

        assert_eq!(refused.kind(), std::io::ErrorKind::AlreadyExists);
        assert_eq!(std::fs::read(&path).unwrap(), b"first");
        assert_eq!(listed(dir.path()), ["key"]);
    }

    #[test]
    fn a_private_folder_is_made_open_to_its_user_alone() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("state");

        private_dir(&path).unwrap();

        assert!(path.is_dir());
        assert_eq!(std::fs::metadata(&path).unwrap().mode() & 0o777, 0o700);
        private_dir(&path).unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn a_folder_already_there_is_private_while_it_is_closed_to_others() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("state");
        std::fs::create_dir(&path).unwrap();
        set_mode(&path, 0o500).unwrap();
        private_dir(&path).unwrap();
        set_mode(&path, 0o750).unwrap();
        assert!(private_dir(&path).is_err());
        assert_eq!(std::fs::metadata(&path).unwrap().mode() & 0o777, 0o750);
    }

    #[cfg(unix)]
    #[test]
    fn a_link_to_a_private_folder_is_not_one() {
        let dir = tempfile::tempdir().unwrap();
        let real = dir.path().join("real");
        private_dir(&real).unwrap();
        let link = dir.path().join("link");
        std::os::unix::fs::symlink(&real, &link).unwrap();

        assert!(private_dir(&link).is_err());
    }
}
