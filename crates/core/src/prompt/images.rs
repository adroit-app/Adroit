//! The images attached to each prompt, the shown tab's among them, and the
//! keys that move among them while one is selected.

use std::collections::{BTreeMap, HashMap};
use std::path::{Path, PathBuf};

use adroit_model::Key;

use crate::content::{Tab, from_kept, kept_name};

/// The attachments belonging to each prompt, including the one shown.
#[derive(Default)]
pub struct Attachments {
    shown: Option<Tab>,
    kept: HashMap<Option<Tab>, Vec<PathBuf>>,
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

    pub fn restore(&mut self, tab: Option<Tab>, images: Vec<PathBuf>) {
        self.kept.entry(tab).or_default().splice(0..0, images);
    }

    pub fn show(&mut self, now: Option<Tab>) {
        if self.shown.is_none()
            && matches!(now, Some(Tab::Draft { .. }))
            && self.kept.get(&now).is_none_or(Vec::is_empty)
        {
            let images = self.take();
            self.kept.insert(now.clone(), images);
        }
        self.shown = now;
    }

    pub fn close(&mut self, now: Option<Tab>) {
        let mut images = self.take();
        self.shown = now;
        images.extend(self.take());
        self.add(images);
    }

    pub fn holding(&self) -> impl Iterator<Item = Tab> + '_ {
        self.kept
            .iter()
            .filter(|(tab, images)| **tab != self.shown && !images.is_empty())
            .filter_map(|(tab, _)| tab.clone())
    }

    pub fn saved(&self) -> BTreeMap<String, Vec<PathBuf>> {
        self.kept
            .iter()
            .filter(|(_, images)| !images.is_empty())
            .filter_map(|(tab, images)| {
                let name = match tab {
                    None => "nothing".into(),
                    Some(tab) => kept_name(tab)?,
                };
                Some((name, images.clone()))
            })
            .collect()
    }

    pub fn restored(saved: BTreeMap<String, Vec<PathBuf>>) -> Self {
        Self {
            shown: None,
            kept: saved
                .into_iter()
                .filter_map(|(name, images)| {
                    let tab = if name == "nothing" {
                        None
                    } else {
                        Some(from_kept(&name)?)
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

pub fn image_key(images: &[PathBuf], selected: Option<&PathBuf>, key: &Key) -> Option<ImageKey> {
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keyboard_selection_moves_removes_and_returns_to_the_prompt() {
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
    fn attachments_written_with_nothing_open_follow_the_next_draft() {
        let mut images = Attachments::default();
        images.add(vec!["/one.png".into()]);
        images.show(Some(Tab::Draft {
            worktree: "/src".into(),
            id: Default::default(),
        }));
        assert_eq!(images.current(), &[PathBuf::from("/one.png")]);
        assert!(images.saved().is_empty());
        images.close(Some(Tab::Session("old".into())));
        images.show(Some(Tab::Session("old".into())));
        assert_eq!(images.current(), &[PathBuf::from("/one.png")]);
    }

    #[test]
    fn removing_all_images_leaves_a_draft_free_to_receive_the_empty_window_s_attachments() {
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
}
