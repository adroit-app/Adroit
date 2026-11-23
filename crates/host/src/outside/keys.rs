//! The keys the user gave programs outside Adroit, which let their holders
//! answer, interrupt and end sessions. Only what tells a key from another is
//! kept, never the key.

use std::path::{Path, PathBuf};
use std::sync::Mutex;

use adroit_model::{MessageId, OutsideKey};
use base64::Engine;
use serde::{Deserialize, Serialize};

/// What a key begins with, so one that leaked is known for what it is.
const PREFIX: &str = "adroit-key-";
const BYTES: usize = 32;
/// The longest name a key goes by, in characters.
const NAME: usize = 64;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct Kept {
    #[serde(flatten)]
    key: OutsideKey,
    /// The SHA-256 of the key, in base64: a key is drawn at random, so nothing
    /// is won by hashing it slowly.
    hash: String,
}

/// One change at a time, each reading the file anew.
static CHANGING: Mutex<()> = Mutex::new(());

pub(crate) fn path() -> PathBuf {
    crate::delegation::app::root().join("outside-keys.json")
}

fn hash(text: &str) -> String {
    let digest = ring::digest::digest(&ring::digest::SHA256, text.as_bytes());
    base64::engine::general_purpose::STANDARD.encode(digest)
}

fn read(path: &Path) -> Result<Vec<Kept>, String> {
    match std::fs::read(path) {
        Ok(bytes) => {
            serde_json::from_slice(&bytes).map_err(|e| format!("cannot read the keys: {e}"))
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Vec::new()),
        Err(e) => Err(format!("cannot read the keys: {e}")),
    }
}

fn write(path: &Path, keys: &[Kept]) -> Result<(), String> {
    let dir = path.parent().ok_or("the keys have no folder")?;
    adroit_sys::fs::private_dir(dir).map_err(|e| e.to_string())?;
    let bytes = serde_json::to_vec_pretty(keys).map_err(|e| e.to_string())?;
    adroit_sys::fs::replace(path, bytes, 0o600).map_err(|e| format!("cannot keep the keys: {e}"))
}

fn changing<T>(
    path: &Path,
    change: impl FnOnce(&mut Vec<Kept>) -> Result<T, String>,
) -> Result<T, String> {
    let _one = CHANGING.lock().unwrap_or_else(|e| e.into_inner());
    let mut keys = read(path)?;
    let changed = change(&mut keys)?;
    write(path, &keys)?;
    Ok(changed)
}

/// The keys there are, oldest first.
pub(crate) fn list(path: &Path) -> Result<Vec<OutsideKey>, String> {
    Ok(read(path)?.into_iter().map(|kept| kept.key).collect())
}

/// A new key for the program called `name`: the key itself, which is told
/// this once, and what is kept of it.
pub(crate) fn issue(path: &Path, name: &str, now_ms: u64) -> Result<(String, OutsideKey), String> {
    let name = name.trim();
    if name.is_empty() || name.chars().count() > NAME || name.chars().any(char::is_control) {
        return Err(format!(
            "a key's name is one line of 1 to {NAME} characters"
        ));
    }
    let drawn: [u8; BYTES] = ring::rand::generate(&ring::rand::SystemRandom::new())
        .map_err(|_| "cannot draw a key".to_string())?
        .expose();
    let encoded = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(drawn);
    let text = format!("{PREFIX}{encoded}");
    let key = OutsideKey {
        id: MessageId::new(),
        name: name.into(),
        issued_ms: now_ms,
        used_ms: None,
    };
    changing(path, |keys| {
        if keys.iter().any(|kept| kept.key.name == key.name) {
            return Err(format!("a key is called {name} already"));
        }
        keys.push(Kept {
            key: key.clone(),
            hash: hash(&text),
        });
        Ok(())
    })?;
    Ok((text, key))
}

pub(crate) fn revoke(path: &Path, id: MessageId) -> Result<(), String> {
    changing(path, |keys| {
        keys.retain(|kept| kept.key.id != id);
        Ok(())
    })
}

/// The name of the program that holds the key `text`, which is marked as
/// used; none for no key there is.
pub(crate) fn holder(path: &Path, text: &str, now_ms: u64) -> Result<Option<String>, String> {
    let hash = hash(text.trim());
    if !read(path)?.iter().any(|kept| kept.hash == hash) {
        return Ok(None);
    }
    changing(path, |keys| {
        Ok(keys.iter_mut().find(|kept| kept.hash == hash).map(|kept| {
            kept.key.used_ms = Some(now_ms);
            kept.key.name.clone()
        }))
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn file() -> (tempfile::TempDir, PathBuf) {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("delegation").join("keys.json");
        (dir, path)
    }

    #[test]
    fn a_key_names_its_holder_and_is_marked_as_used() {
        let (_dir, path) = file();
        let (text, _) = issue(&path, " Famulen ", 1).unwrap();
        let (other, _) = issue(&path, "Other", 2).unwrap();
        assert_ne!(text, other);
        assert_eq!(
            holder(&path, &format!("{text}\n"), 5).unwrap().as_deref(),
            Some("Famulen")
        );
        let kept = list(&path).unwrap();
        assert_eq!(
            kept.iter().map(|k| k.used_ms).collect::<Vec<_>>(),
            [Some(5), None]
        );
        for none in ["", "adroit-key-", &text[..text.len() - 1]] {
            assert_eq!(holder(&path, none, 6).unwrap(), None, "{none:?}");
        }
    }

    #[test]
    fn a_key_revoked_lets_no_one_in() {
        let (_dir, path) = file();
        let (text, key) = issue(&path, "Famulen", 1).unwrap();
        revoke(&path, key.id).unwrap();
        assert_eq!(holder(&path, &text, 2).unwrap(), None);
        assert!(list(&path).unwrap().is_empty());
    }

    #[test]
    fn the_key_itself_is_kept_nowhere() {
        let (_dir, path) = file();
        let (text, _) = issue(&path, "Famulen", 1).unwrap();
        let kept = std::fs::read_to_string(&path).unwrap();
        assert!(!kept.contains(text.trim_start_matches(PREFIX)), "{kept}");
    }

    #[cfg(unix)]
    #[test]
    fn the_keys_are_open_to_the_user_alone() {
        use std::os::unix::fs::PermissionsExt;
        let (_dir, path) = file();
        issue(&path, "Famulen", 1).unwrap();
        let mode = |path: &Path| path.metadata().unwrap().permissions().mode() & 0o777;
        assert_eq!((mode(&path), mode(path.parent().unwrap())), (0o600, 0o700));
    }

    #[test]
    fn a_key_needs_a_one_line_name_no_other_key_has() {
        let (_dir, path) = file();
        issue(&path, "Famulen", 1).unwrap();
        for name in ["", " ", "a\nb", "Famulen", &"n".repeat(NAME + 1)] {
            assert!(issue(&path, name, 2).is_err(), "{name:?}");
        }
        assert_eq!(list(&path).unwrap().len(), 1);
    }

    #[test]
    fn keys_that_cannot_be_read_let_no_one_in_and_are_left_as_they_are() {
        let (_dir, path) = file();
        issue(&path, "Famulen", 1).unwrap();
        std::fs::write(&path, "broken").unwrap();
        assert!(holder(&path, "x", 2).is_err());
        assert!(issue(&path, "Other", 2).is_err());
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "broken");
    }
}
