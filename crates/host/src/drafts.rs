//! What is written in the windows' prompts, kept in the instance's state
//! folder from one run to the next. Each window sends what it changed, and
//! the changes are made one at a time to what is kept, so windows writing
//! at once lose nothing of each other's.

use std::path::Path;
use std::sync::Mutex;

use adroit_protocol::{DraftChanges, Drafts};

/// Taken while what is kept is read, changed and written again.
static KEEPING: Mutex<()> = Mutex::new(());

fn read<T: serde::de::DeserializeOwned>(path: &Path) -> Result<Option<T>, String> {
    match std::fs::read(path) {
        Ok(bytes) => serde_json::from_slice(&bytes)
            .map(Some)
            .map_err(|error| format!("cannot read {}: {error}", path.display())),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(format!("cannot read {}: {error}", path.display())),
    }
}

/// What is kept in `directory`. Files an earlier adroit kept apart are read
/// until the drafts are first kept; a checkpoint that cannot be read is an
/// error, never masked by them.
pub fn load(directory: &Path) -> Result<Drafts, String> {
    if let Some(drafts) = read(&directory.join("draft-checkpoint.json"))? {
        return Ok(drafts);
    }
    Ok(Drafts {
        text: read(&directory.join("drafts.json"))?.unwrap_or_default(),
        images: read(&directory.join("attachments.json"))?.unwrap_or_default(),
    })
}

/// Make `changes` to what is kept in `directory`. What cannot be read is left
/// as it is rather than written over.
pub fn change(directory: &Path, changes: DraftChanges) -> Result<(), String> {
    let _keeping = KEEPING
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let mut drafts = load(directory)?;
    drafts.apply(changes);
    keep(directory, &drafts)
}

/// Write `drafts` in `directory` whole, or not at all.
fn keep(directory: &Path, drafts: &Drafts) -> Result<(), String> {
    use std::io::Write;
    use std::os::unix::fs::OpenOptionsExt;

    let temporary = directory.join(format!(
        ".draft-checkpoint-{}.tmp",
        adroit_model::DraftId::new()
    ));
    let result = (|| {
        std::fs::create_dir_all(directory).map_err(|error| error.to_string())?;
        let mut file = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(&temporary)
            .map_err(|error| error.to_string())?;
        file.write_all(&serde_json::to_vec(drafts).map_err(|error| error.to_string())?)
            .map_err(|error| error.to_string())?;
        file.sync_all().map_err(|error| error.to_string())?;
        std::fs::rename(&temporary, directory.join("draft-checkpoint.json"))
            .map_err(|error| error.to_string())
    })();
    if result.is_err() {
        let _ = std::fs::remove_file(&temporary);
    }
    result
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;
    use std::path::PathBuf;

    use super::*;

    fn written(key: &str, text: &str) -> DraftChanges {
        DraftChanges {
            text: BTreeMap::from([(key.to_string(), Some(text.to_string()))]),
            ..Default::default()
        }
    }

    #[test]
    fn a_failed_write_leaves_what_is_there_and_can_be_made_again() {
        let directory = tempfile::tempdir().unwrap();
        let blocked = directory.path().join("not-a-directory");
        std::fs::write(&blocked, "Keep me").unwrap();
        assert!(change(&blocked, written("session:a", "fix")).is_err());
        assert_eq!(std::fs::read_to_string(&blocked).unwrap(), "Keep me");
        change(directory.path(), written("session:a", "fix")).unwrap();
        assert_eq!(
            load(directory.path()).unwrap().text.get("session:a"),
            Some(&"fix".to_string())
        );
    }

    #[test]
    fn changes_from_two_windows_keep_each_other_s() {
        let directory = tempfile::tempdir().unwrap();
        change(directory.path(), written("session:a", "first")).unwrap();
        change(directory.path(), written("session:b", "second")).unwrap();
        let kept = load(directory.path()).unwrap();
        assert_eq!(kept.text.len(), 2);
    }

    #[test]
    fn legacy_files_are_read_only_until_a_checkpoint_exists_and_never_mask_a_broken_checkpoint() {
        let directory = tempfile::tempdir().unwrap();
        std::fs::write(
            directory.path().join("drafts.json"),
            r#"{"session:old":"legacy draft"}"#,
        )
        .unwrap();
        std::fs::write(
            directory.path().join("attachments.json"),
            r#"{"session:old":["/legacy.png"]}"#,
        )
        .unwrap();
        let checkpoint = load(directory.path()).unwrap();
        assert_eq!(
            checkpoint.text.get("session:old").map(String::as_str),
            Some("legacy draft")
        );
        assert_eq!(
            checkpoint.images.get("session:old"),
            Some(&vec![PathBuf::from("/legacy.png")])
        );
        std::fs::write(
            directory.path().join("draft-checkpoint.json"),
            "incomplete checkpoint",
        )
        .unwrap();
        assert!(load(directory.path()).is_err());
        assert!(
            change(directory.path(), written("session:a", "fix")).is_err(),
            "a checkpoint that cannot be read is not written over"
        );
        assert_eq!(
            std::fs::read_to_string(directory.path().join("draft-checkpoint.json")).unwrap(),
            "incomplete checkpoint"
        );
    }
}
