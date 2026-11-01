//! This computer's key bindings file, which the window binds its keys by in
//! every environment it shows.

use std::path::{Path, PathBuf};

use adroit_protocol::{ConfigFile, FileText};

/// What a keys file made without a binding in it starts with.
pub(crate) const STARTER: &str = "# Key bindings over the defaults, area by area.\n";

pub(crate) fn path() -> PathBuf {
    adroit_model::paths::keys_path()
}

/// The file as it is now.
pub(crate) fn read() -> ConfigFile {
    read_at(&path())
}

/// Make `text` the key bindings.
pub(crate) fn save(text: &str) -> Result<(), String> {
    save_at(&path(), text)
}

fn read_at(path: &Path) -> ConfigFile {
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

fn save_at(path: &Path, text: &str) -> Result<(), String> {
    let failed = |e: std::io::Error| format!("cannot write {}: {e}", path.display());
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).map_err(failed)?;
    }
    let text = if text.trim().is_empty() && !path.exists() {
        STARTER
    } else {
        text
    };
    std::fs::write(path, text).map_err(failed)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_bindings_saved_are_read_back_and_a_file_missing_reads_as_missing() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("adroit").join("keys.toml");
        assert_eq!(read_at(&path).text, FileText::Missing);
        save_at(&path, "[tree]\n\"J\" = \"cursor.down\"\n").unwrap();
        assert_eq!(
            read_at(&path),
            ConfigFile {
                path: path.clone(),
                text: FileText::Read {
                    text: "[tree]\n\"J\" = \"cursor.down\"\n".into()
                },
            }
        );
    }

    #[test]
    fn a_file_made_with_no_binding_starts_with_a_note_of_what_it_holds() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("keys.toml");
        save_at(&path, "").unwrap();
        assert_eq!(
            read_at(&path).text,
            FileText::Read {
                text: STARTER.into()
            }
        );
    }
}
