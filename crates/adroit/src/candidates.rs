//! Evidence for declared candidate outputs and verification inputs.
use std::{
    ffi::{CStr, CString},
    fs::{File, OpenOptions},
    io::{self, Read},
    os::{
        fd::{AsRawFd, FromRawFd},
        unix::fs::{MetadataExt, OpenOptionsExt},
    },
    path::{Component, Path},
};

use adroit_model::{CandidateFile, CandidateFileState, CandidateFiles};
use sha2::{Digest, Sha256};

type Result<T> = std::result::Result<T, String>;
const MAX_FILES: usize = 16_384;
const MAX_BYTES: u64 = 128 * 1024 * 1024;

fn hex(bytes: &[u8]) -> String {
    const DIGITS: &[u8; 16] = b"0123456789abcdef";
    let mut value = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        value.push(DIGITS[(byte >> 4) as usize] as char);
        value.push(DIGITS[(byte & 15) as usize] as char);
    }
    value
}

fn open_at(parent: &File, name: &CStr, flags: i32) -> io::Result<File> {
    // SAFETY: the directory descriptor and the NUL-terminated name stay valid for this call.
    let fd = unsafe { libc::openat(parent.as_raw_fd(), name.as_ptr(), flags | libc::O_CLOEXEC) };
    if fd < 0 {
        return Err(io::Error::last_os_error());
    }
    // SAFETY: openat returned a fresh descriptor, now owned exclusively by this File.
    Ok(unsafe { File::from_raw_fd(fd) })
}

fn link_at(parent: &File, name: &CStr) -> io::Result<String> {
    let mut buffer = vec![0_u8; 16 * 1024];
    // SAFETY: the name is NUL-terminated and the writable buffer has the stated length.
    let length = unsafe {
        libc::readlinkat(
            parent.as_raw_fd(),
            name.as_ptr(),
            buffer.as_mut_ptr().cast(),
            buffer.len(),
        )
    };
    if length < 0 {
        return Err(io::Error::last_os_error());
    }
    if length as usize == buffer.len() {
        return Err(io::Error::other("the link target was truncated"));
    }
    buffer.truncate(length as usize);
    String::from_utf8(buffer).map_err(|_| io::Error::other("the link target is not UTF-8"))
}

fn stamp(metadata: &std::fs::Metadata) -> (u64, u64, u64, u32, i64, i64, i64, i64) {
    (
        metadata.dev(),
        metadata.ino(),
        metadata.len(),
        metadata.mode(),
        metadata.mtime(),
        metadata.mtime_nsec(),
        metadata.ctime(),
        metadata.ctime_nsec(),
    )
}

fn open_root(root: &Path) -> Result<File> {
    if !root.is_absolute() {
        return Err("the candidate working area must be absolute".into());
    }
    let mut directory = OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_DIRECTORY | libc::O_NOFOLLOW)
        .open("/")
        .map_err(|error| format!("cannot open the filesystem root: {error}"))?;
    for component in root.components() {
        let name = match component {
            Component::RootDir => continue,
            Component::Normal(name) => name,
            _ => return Err("the candidate working area must have no relative components".into()),
        };
        use std::os::unix::ffi::OsStrExt;
        let name = CString::new(name.as_bytes())
            .map_err(|_| "the candidate working-area path contains NUL")?;
        directory = open_at(
            &directory,
            &name,
            libc::O_RDONLY | libc::O_DIRECTORY | libc::O_NOFOLLOW,
        )
        .map_err(|error| {
            format!("cannot open the candidate working area without following links: {error}")
        })?;
    }
    Ok(directory)
}

fn capture_one(root: &File, path: &str, remaining: &mut u64) -> Result<CandidateFileState> {
    let parts: Vec<_> = Path::new(path).components().collect();
    if path.split('/').any(|name| {
        name.is_empty()
            || name == "."
            || name == ".."
            || name.eq_ignore_ascii_case(".git")
            || name.eq_ignore_ascii_case(".jj")
    }) || parts.is_empty()
        || parts.iter().any(
            |part| !matches!(part,Component::Normal(name) if *name != ".git" && *name != ".jj"),
        )
    {
        return Err("manifest paths must be relative files outside VCS metadata".into());
    }
    let mut parent = root.try_clone().map_err(|error| error.to_string())?;
    let mut names = Vec::new();
    for part in parts {
        let Component::Normal(name) = part else {
            unreachable!()
        };
        use std::os::unix::ffi::OsStrExt;
        names.push(CString::new(name.as_bytes()).map_err(|_| "a manifest path contains NUL")?);
    }
    for name in &names[..names.len() - 1] {
        parent = match open_at(
            &parent,
            name,
            libc::O_RDONLY | libc::O_DIRECTORY | libc::O_NOFOLLOW,
        ) {
            Ok(directory) => directory,
            Err(error) if error.kind() == io::ErrorKind::NotFound => {
                return Ok(CandidateFileState::Missing);
            }
            Err(error) => {
                return Err(format!(
                    "cannot open manifest parent without following links: {error}"
                ));
            }
        };
    }
    let name = names.last().expect("nonempty path");
    match link_at(&parent, name) {
        Ok(target) => return Ok(CandidateFileState::Symlink { target }),
        Err(error) if error.kind() == io::ErrorKind::NotFound => {
            return Ok(CandidateFileState::Missing);
        }
        Err(error) if error.raw_os_error() == Some(libc::EINVAL) => {}
        Err(error) => return Err(format!("cannot inspect manifest link: {error}")),
    }
    let mut file = open_at(
        &parent,
        name,
        libc::O_RDONLY | libc::O_NOFOLLOW | libc::O_NONBLOCK,
    )
    .map_err(|error| format!("cannot open manifest file without following links: {error}"))?;
    let before = file.metadata().map_err(|error| error.to_string())?;
    if !before.is_file() {
        return Err("a manifest entry must be a file, symlink or deleted file".into());
    }
    if before.len() > *remaining {
        return Err("the declared candidate files exceed 128 MiB".into());
    }
    let mut digest = Sha256::new();
    let mut buffer = [0_u8; 64 * 1024];
    loop {
        let length = file.read(&mut buffer).map_err(|error| error.to_string())?;
        if length == 0 {
            break;
        }
        *remaining = remaining
            .checked_sub(length as u64)
            .ok_or("the declared candidate files exceed 128 MiB")?;
        digest.update(&buffer[..length]);
    }
    if stamp(&before) != stamp(&file.metadata().map_err(|error| error.to_string())?) {
        return Err("a manifest file changed while its evidence was read; retry capture".into());
    }
    Ok(CandidateFileState::File {
        sha256: hex(&digest.finalize()),
        executable: before.mode() & 0o111 != 0,
    })
}

pub fn capture_files(root: &Path, paths: &[String]) -> Result<CandidateFiles> {
    if paths.is_empty() || paths.len() > MAX_FILES {
        return Err("a candidate manifest needs between 1 and 16384 file paths".into());
    }
    let directory = open_root(root)?;
    let mut paths = paths.to_vec();
    paths.sort();
    if paths.windows(2).any(|pair| pair[0] == pair[1]) {
        return Err("a candidate manifest lists a path more than once".into());
    }
    let mut remaining = MAX_BYTES;
    let files: Vec<_> = paths
        .into_iter()
        .map(|path| {
            capture_one(&directory, &path, &mut remaining)
                .map(|state| CandidateFile { path, state })
        })
        .collect::<Result<_>>()?;
    let mut digest = Sha256::new();
    digest.update(b"adroit-candidate-files-v1\n");
    digest.update(serde_json::to_vec(&files).map_err(|error| error.to_string())?);
    Ok(CandidateFiles {
        version: 1,
        fingerprint: format!("sha256:{}", hex(&digest.finalize())),
        files,
    })
}

mod native;
pub use native::capture_candidate;
