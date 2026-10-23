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

/// The attachments belonging to each prompt, including the one shown.
#[derive(Default)]
pub struct Attachments {
    shown: Option<crate::content::Tab>,
    kept: std::collections::HashMap<Option<crate::content::Tab>, Vec<PathBuf>>,
}

impl Attachments {
    pub fn current(&self) -> &[PathBuf] {
        self.kept
            .get(&self.shown)
            .map(Vec::as_slice)
            .unwrap_or_default()
    }

    pub fn add(&mut self, images: Vec<PathBuf>) {
        self.kept
            .entry(self.shown.clone())
            .or_default()
            .extend(images);
    }

    pub fn remove(&mut self, path: &Path) {
        if let Some(images) = self.kept.get_mut(&self.shown) {
            images.retain(|image| image != path);
        }
    }

    pub fn take(&mut self) -> Vec<PathBuf> {
        self.kept.remove(&self.shown).unwrap_or_default()
    }

    pub fn restore(&mut self, tab: Option<crate::content::Tab>, images: Vec<PathBuf>) {
        self.kept.entry(tab).or_default().splice(0..0, images);
    }

    pub fn show(&mut self, now: Option<crate::content::Tab>) {
        if self.shown.is_none()
            && matches!(now, Some(crate::content::Tab::Draft { .. }))
            && self.kept.get(&now).is_none_or(Vec::is_empty)
        {
            let images = self.take();
            self.kept.insert(now.clone(), images);
        }
        self.shown = now;
    }

    pub fn close(&mut self, now: Option<crate::content::Tab>) {
        let mut images = self.take();
        self.shown = now;
        images.extend(self.take());
        self.add(images);
    }

    pub fn follow(&mut self, sessions: &[adroit_model::Session]) {
        let moved = |tab: Option<crate::content::Tab>| match tab {
            Some(crate::content::Tab::Session(ref id)) => crate::nav::successor(sessions, id)
                .map(|id| Some(crate::content::Tab::Session(id.into())))
                .unwrap_or(tab),
            _ => tab,
        };
        self.shown = moved(self.shown.take());
        self.kept = std::mem::take(&mut self.kept)
            .into_iter()
            .map(|(tab, images)| (moved(tab), images))
            .collect();
    }

    pub fn holding(&self) -> impl Iterator<Item = crate::content::Tab> + '_ {
        self.kept
            .iter()
            .filter(|(tab, images)| **tab != self.shown && !images.is_empty())
            .filter_map(|(tab, _)| tab.clone())
    }

    pub fn saved(&self) -> std::collections::BTreeMap<String, Vec<PathBuf>> {
        self.kept
            .iter()
            .filter(|(_, images)| !images.is_empty())
            .filter_map(|(tab, images)| {
                let name = match tab {
                    None => "nothing".into(),
                    Some(tab) => crate::content::kept_name(tab)?,
                };
                Some((name, images.clone()))
            })
            .collect()
    }

    pub fn restored(saved: std::collections::BTreeMap<String, Vec<PathBuf>>) -> Self {
        Self {
            shown: None,
            kept: saved
                .into_iter()
                .filter_map(|(name, images)| {
                    let tab = if name == "nothing" {
                        None
                    } else {
                        Some(crate::content::from_kept(&name)?)
                    };
                    Some((tab, images))
                })
                .collect(),
        }
    }
}

#[derive(Debug, PartialEq, Eq)]
pub enum ImageKey {
    Select(Option<PathBuf>),
    Remove(PathBuf),
}

pub fn image_key(
    images: &[PathBuf],
    selected: Option<&PathBuf>,
    key: &adroit_term::Key,
) -> Option<ImageKey> {
    if key.ctrl || key.alt || key.meta {
        return None;
    }
    let index = images.iter().position(|path| Some(path) == selected)?;
    Some(match key.key.as_str() {
        "ArrowLeft" | "h" => ImageKey::Select(images.get(index.saturating_sub(1)).cloned()),
        "ArrowRight" | "l" => {
            ImageKey::Select(images.get((index + 1).min(images.len() - 1)).cloned())
        }
        "Tab" => ImageKey::Select(
            images
                .get(if key.shift {
                    (index + images.len() - 1) % images.len()
                } else {
                    (index + 1) % images.len()
                })
                .cloned(),
        ),
        "Delete" | "Backspace" | "x" => ImageKey::Remove(images[index].clone()),
        "Escape" | "Enter" => ImageKey::Select(None),
        _ => return None,
    })
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
    fn keyboard_selection_moves_removes_and_returns_to_the_prompt() {
        use adroit_term::Key;
        let images = vec![PathBuf::from("/one.png"), PathBuf::from("/two.png")];
        let selected = Some(&images[0]);
        assert_eq!(
            image_key(&images, selected, &Key::new("ArrowRight")),
            Some(ImageKey::Select(Some(images[1].clone())))
        );
        assert_eq!(
            image_key(&images, Some(&images[1]), &Key::new("Tab")),
            Some(ImageKey::Select(Some(images[0].clone())))
        );
        for key in ["Delete", "Backspace", "x"] {
            assert_eq!(
                image_key(&images, selected, &Key::new(key)),
                Some(ImageKey::Remove(images[0].clone()))
            );
        }
        assert_eq!(
            image_key(&images, selected, &Key::new("Escape")),
            Some(ImageKey::Select(None))
        );
        assert_eq!(
            image_key(&images, selected, &Key::new("Enter").ctrl()),
            None
        );
        assert_eq!(image_key(&[], selected, &Key::new("Delete")), None);
        assert_eq!(image_key(&images, None, &Key::new("Delete")), None);
    }

    #[test]
    fn a_failed_send_restores_images_to_its_tab_before_any_new_attachments() {
        use crate::content::Tab;
        let a = Some(Tab::Session("a".into()));
        let b = Some(Tab::Session("b".into()));
        let mut images = Attachments::default();
        images.show(a.clone());
        images.add(vec!["/one.png".into()]);
        let sent = images.take();
        images.add(vec!["/two.png".into()]);
        images.show(b);
        images.restore(a.clone(), sent);
        assert!(images.current().is_empty());
        images.show(a);
        assert_eq!(
            images.current(),
            &[PathBuf::from("/one.png"), PathBuf::from("/two.png")]
        );
    }

    #[test]
    fn attachments_written_with_nothing_open_follow_the_next_draft_and_session_successors() {
        use crate::content::Tab;
        let mut images = Attachments::default();
        images.add(vec!["/one.png".into()]);
        images.show(Some(Tab::Draft {
            worktree: "/src".into(),
            id: Default::default(),
        }));
        assert_eq!(images.current(), &[PathBuf::from("/one.png")]);
        assert!(images.saved().is_empty());
        images.close(Some(Tab::Session("old".into())));
        images.follow(&[adroit_model::Session {
            id: "new".into(),
            session_id: "old".into(),
            ..Default::default()
        }]);
        images.show(Some(Tab::Session("new".into())));
        assert_eq!(images.current(), &[PathBuf::from("/one.png")]);
    }

    #[test]
    fn removing_all_images_leaves_a_draft_free_to_receive_the_empty_window_s_attachments() {
        use crate::content::Tab;
        let draft = Some(Tab::Draft {
            worktree: "/src".into(),
            id: Default::default(),
        });
        let mut images = Attachments::default();
        images.show(draft.clone());
        images.add(vec!["/old.png".into()]);
        images.remove(Path::new("/old.png"));
        images.show(None);
        images.add(vec!["/new.png".into()]);
        images.show(draft);
        assert_eq!(images.current(), &[PathBuf::from("/new.png")]);
    }

    #[test]
    fn attachments_follow_tabs_close_with_the_prompt_and_survive_reopening() {
        use crate::content::Tab;
        let a = Some(Tab::Session("a".into()));
        let b = Some(Tab::Session("b".into()));
        let mut images = Attachments::default();
        images.show(a.clone());
        images.add(vec!["/a.png".into()]);
        images.show(b.clone());
        images.add(vec!["/b.png".into()]);
        let mut restored = Attachments::restored(images.saved());
        restored.show(a.clone());
        assert_eq!(restored.current(), &[PathBuf::from("/a.png")]);
        images.close(a);
        assert_eq!(
            images.current(),
            &[PathBuf::from("/b.png"), PathBuf::from("/a.png")]
        );
        assert_eq!(images.take().len(), 2);
        assert!(images.current().is_empty());
    }

    #[test]
    fn removing_an_attachment_leaves_the_other_images_in_order() {
        let mut images = Attachments::default();
        images.add(vec!["/one.png".into(), "/two.png".into()]);
        images.remove(Path::new("/one.png"));
        assert_eq!(images.current(), &[PathBuf::from("/two.png")]);
        assert_eq!(images.saved().len(), 1);
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
