//! What the host reads of the user's configuration: `settings.toml` and
//! `keys.toml` in adroit's configuration folder, as they are on disk.

use std::path::Path;

use adroit_protocol::{ConfigFile, ConfigWork, FileText};

/// What `path` holds now.
pub fn file(path: &Path) -> ConfigFile {
    let text = match std::fs::read_to_string(path) {
        Ok(text) => FileText::Read { text },
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => FileText::Missing,
        Err(error) => FileText::Unreadable {
            error: error.to_string(),
        },
    };
    ConfigFile {
        path: path.to_path_buf(),
        text,
    }
}

/// Both files, read now.
pub fn read() -> ConfigWork {
    ConfigWork {
        settings: Some(file(&crate::settings::path())),
        keys: Some(file(&crate::paths::keys_path())),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_file_is_read_missing_or_unreadable_as_it_is_on_disk() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("keys.toml");
        assert_eq!(file(&path).text, FileText::Missing);
        std::fs::write(&path, "[tree]\n").unwrap();
        assert_eq!(
            file(&path),
            ConfigFile {
                path: path.clone(),
                text: FileText::Read {
                    text: "[tree]\n".into()
                },
            }
        );
        assert!(matches!(
            file(directory.path()).text,
            FileText::Unreadable { .. }
        ));
    }
}
