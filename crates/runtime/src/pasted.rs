//! Images kept separately from the words written in each prompt.

use std::fmt;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use base64::Engine;

use crate::paths::state_dir;

/// How long a pasted image is kept: a prompt can be set aside and sent
/// later, but the receiving agent has its own copy once it is sent.
pub const KEPT_FOR: Duration = Duration::from_secs(7 * 24 * 60 * 60);

/// Where pasted images are kept.
pub fn dir() -> PathBuf {
    state_dir().join("adroit").join("pasted")
}

#[derive(Debug)]
pub enum Error {
    /// An image outside the supported attachment formats.
    Unsupported(String),
    Undecodable,
    Io(std::io::Error),
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::Unsupported(mime) => {
                write!(f, "Images must be PNG, JPEG, GIF or WebP, not {mime}")
            }
            Error::Undecodable => write!(f, "The pasted image could not be read"),
            Error::Io(e) => write!(f, "The pasted image could not be saved: {e}"),
        }
    }
}

/// The file extension for a supported image MIME type.
fn extension(mime: &str) -> Option<&'static str> {
    match mime.to_ascii_lowercase().as_str() {
        "image/png" => Some("png"),
        "image/jpeg" | "image/jpg" => Some("jpg"),
        "image/gif" => Some("gif"),
        "image/webp" => Some("webp"),
        _ => None,
    }
}

/// Write the image of `mime`, given in base64, as a new file in `dir`,
/// named after `now`, and return its path. A pasted image is often a
/// screenshot, so the file and a `dir` made for it are the user's alone.
pub fn save(dir: &Path, mime: &str, base64: &str, now: SystemTime) -> Result<PathBuf, Error> {
    use std::os::unix::fs::{DirBuilderExt, OpenOptionsExt};
    let ext = extension(mime).ok_or_else(|| Error::Unsupported(mime.to_string()))?;
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(base64.trim())
        .map_err(|_| Error::Undecodable)?;
    std::fs::DirBuilder::new()
        .recursive(true)
        .mode(0o700)
        .create(dir)
        .map_err(Error::Io)?;
    let millis = now
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis();
    for n in 0.. {
        let name = match n {
            0 => format!("{millis}.{ext}"),
            n => format!("{millis}-{n}.{ext}"),
        };
        let path = dir.join(name);
        let created = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(&path);
        match created {
            Ok(mut file) => {
                file.write_all(&bytes).map_err(Error::Io)?;
                return Ok(path);
            }
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(e) => return Err(Error::Io(e)),
        }
    }
    unreachable!("some name is free")
}

/// Remove the images in `dir` pasted longer than [`KEPT_FOR`] before `now`.
pub fn prune(dir: &Path, now: SystemTime) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let old = entry
            .metadata()
            .and_then(|m| m.modified())
            .is_ok_and(|at| now.duration_since(at).is_ok_and(|age| age > KEPT_FOR));
        if old {
            let _ = std::fs::remove_file(entry.path());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const PNG_BASE64: &str = "iVBORw0KGgo=";

    fn at(secs: u64) -> SystemTime {
        UNIX_EPOCH + Duration::from_secs(secs)
    }

    #[test]
    fn a_pasted_image_is_saved_with_its_bytes_under_an_extension_for_its_kind() {
        let dir = tempfile::tempdir().unwrap();

        let path = save(dir.path(), "image/png", PNG_BASE64, at(1_000)).unwrap();

        assert_eq!(path.parent(), Some(dir.path()));
        assert_eq!(path.extension().unwrap(), "png");
        assert_eq!(std::fs::read(&path).unwrap(), b"\x89PNG\r\n\x1a\n");
    }

    #[test]
    fn a_pasted_image_and_the_folder_it_is_kept_in_are_open_to_no_one_else() {
        use std::os::unix::fs::PermissionsExt;
        let root = tempfile::tempdir().unwrap();
        let dir = root.path().join("pasted");

        let path = save(&dir, "image/png", PNG_BASE64, at(1)).unwrap();

        let mode = |p: &Path| std::fs::metadata(p).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode(&dir), 0o700);
        assert_eq!(mode(&path), 0o600);
    }

    #[test]
    fn claude_code_s_kinds_of_image_are_taken_and_no_other() {
        let dir = tempfile::tempdir().unwrap();
        for (mime, ext) in [
            ("image/png", "png"),
            ("image/jpeg", "jpg"),
            ("image/gif", "gif"),
            ("image/webp", "webp"),
        ] {
            let path = save(dir.path(), mime, PNG_BASE64, at(1)).unwrap();
            assert_eq!(path.extension().unwrap(), ext, "{mime}");
        }
        for mime in ["image/tiff", "image/heic", "video/mp4"] {
            assert!(
                matches!(
                    save(dir.path(), mime, PNG_BASE64, at(1)),
                    Err(Error::Unsupported(_))
                ),
                "{mime}"
            );
        }
    }

    #[test]
    fn images_pasted_at_the_same_moment_are_kept_apart() {
        let dir = tempfile::tempdir().unwrap();

        let first = save(dir.path(), "image/png", PNG_BASE64, at(5)).unwrap();
        let second = save(dir.path(), "image/png", PNG_BASE64, at(5)).unwrap();

        assert_ne!(first, second);
        assert!(first.exists() && second.exists());
    }

    #[test]
    fn bytes_that_are_not_base64_are_refused_and_nothing_is_written() {
        let dir = tempfile::tempdir().unwrap();

        let saved = save(dir.path(), "image/png", "not base64!", at(1));

        assert!(matches!(saved, Err(Error::Undecodable)));
        assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 0);
    }

    #[test]
    fn images_older_than_they_are_kept_for_are_removed_and_newer_ones_stay() {
        let dir = tempfile::tempdir().unwrap();
        let now = SystemTime::now();
        let old = save(dir.path(), "image/png", PNG_BASE64, at(1)).unwrap();
        let recent = save(dir.path(), "image/png", PNG_BASE64, at(2)).unwrap();
        std::fs::File::options()
            .write(true)
            .open(&old)
            .unwrap()
            .set_modified(now - KEPT_FOR - Duration::from_secs(60))
            .unwrap();

        prune(dir.path(), now);

        assert!(!old.exists());
        assert!(recent.exists());
    }
}
