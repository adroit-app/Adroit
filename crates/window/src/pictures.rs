//! The images a session's console drew, kept after they scroll away, each
//! with a small copy to show in the overview.

use std::collections::HashMap;
use std::hash::{Hash, Hasher};
use std::sync::Arc;

use adroit_core::details::Shot;
use adroit_term::Drawn;
use adroit_term::picture::{data_url, png, thumb};
use image::{DynamicImage, RgbaImage};

/// The most images kept for one session: its newest.
const PER_SESSION: usize = 30;

/// The most bytes the kept images take, all sessions together; past it the
/// oldest give way.
const BYTES: usize = 128 * 1024 * 1024;

/// An image the console drew, kept.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Kept {
    /// What it shows, hashed: the same image drawn again is kept once.
    pub id: u64,
    pub at_ms: u64,
    pub width: u32,
    pub height: u32,
    pub png: Arc<[u8]>,
    pub thumb: Arc<str>,
}

/// What `drawn` shows, hashed, as [`Kept::id`] tells it.
pub fn id_of(drawn: &Drawn) -> u64 {
    let mut h = std::hash::DefaultHasher::new();
    (drawn.width, drawn.height, &drawn.rgba).hash(&mut h);
    h.finish()
}

impl Kept {
    /// `drawn` encoded to keep, seen at `at_ms`; `None` when its pixels do
    /// not make an image.
    pub fn of(drawn: Drawn, at_ms: u64) -> Option<Kept> {
        let id = id_of(&drawn);
        let image =
            DynamicImage::ImageRgba8(RgbaImage::from_raw(drawn.width, drawn.height, drawn.rgba)?);
        Some(Kept {
            id,
            at_ms,
            width: image.width(),
            height: image.height(),
            png: png(&image)?.into(),
            thumb: thumb(&image)?,
        })
    }

    fn bytes(&self) -> usize {
        self.png.len() + self.thumb.len()
    }
}

/// The images each session's console drew, newest first, within bounds.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Store {
    sessions: HashMap<String, Vec<Kept>>,
    per_session: usize,
    most_bytes: usize,
    bytes: usize,
}

impl Default for Store {
    fn default() -> Self {
        Store::new(PER_SESSION, BYTES)
    }
}

impl Store {
    pub fn new(per_session: usize, most_bytes: usize) -> Store {
        Store {
            sessions: HashMap::new(),
            per_session,
            most_bytes,
            bytes: 0,
        }
    }

    /// Keep `kept` for `session`, letting the oldest go past the bounds;
    /// whether it was not kept already. One kept already stays where it is.
    pub fn keep(&mut self, session: &str, kept: Kept) -> bool {
        let list = self.sessions.entry(session.to_string()).or_default();
        if list.iter().any(|k| k.id == kept.id) {
            return false;
        }
        self.bytes += kept.bytes();
        list.insert(0, kept);
        while list.len() > self.per_session {
            if let Some(gone) = list.pop() {
                self.bytes -= gone.bytes();
            }
        }
        while self.bytes > self.most_bytes {
            let oldest = self
                .sessions
                .iter()
                .filter_map(|(s, l)| Some((l.last()?.at_ms, s.clone())))
                .min();
            let Some((_, s)) = oldest else { break };
            if let Some(gone) = self.sessions.get_mut(&s).and_then(Vec::pop) {
                self.bytes -= gone.bytes();
            }
        }
        self.sessions.retain(|_, l| !l.is_empty());
        true
    }

    /// The images kept for `session`, newest first.
    pub fn of(&self, session: &str) -> &[Kept] {
        self.sessions.get(session).map_or(&[], Vec::as_slice)
    }

    pub fn find(&self, session: &str, id: u64) -> Option<&Kept> {
        self.of(session).iter().find(|k| k.id == id)
    }

    /// The sessions images are kept for.
    pub fn sessions(&self) -> impl Iterator<Item = &str> {
        self.sessions.keys().map(String::as_str)
    }

    /// Let the images of sessions not among `live` go.
    pub fn retain(&mut self, live: impl Fn(&str) -> bool) {
        let mut freed = 0;
        self.sessions.retain(|s, l| {
            let keep = live(s);
            if !keep {
                freed += l.iter().map(Kept::bytes).sum::<usize>();
            }
            keep
        });
        self.bytes -= freed;
    }
}

/// What the overview's images are made from: the console's images kept.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Gallery {
    pub store: Store,
}

/// What `session`'s console drew, kept in `store`, as the overview lists it.
pub fn drawn(store: &Store, session: &str) -> Vec<Shot> {
    store
        .of(session)
        .iter()
        .map(|k| Shot::console(k.id, k.width, k.height, k.at_ms, Some(k.thumb.clone())))
        .collect()
}

/// A kept image, to show whole.
pub fn kept_shown(kept: &Kept) -> Arc<str> {
    data_url("image/png", &kept.png)
}

#[cfg(test)]
mod tests {
    use base64::Engine;

    use super::*;

    fn pixels(width: u32, height: u32, shade: u8) -> Drawn {
        Drawn {
            width,
            height,
            rgba: [shade, 0, 0, 255].repeat((width * height) as usize),
        }
    }
    fn kept(shade: u8, at_ms: u64) -> Kept {
        Kept::of(pixels(2, 2, shade), at_ms).unwrap()
    }
    fn ids(store: &Store, session: &str) -> Vec<u64> {
        store.of(session).iter().map(|k| k.at_ms).collect()
    }
    fn small_size(url: &str) -> (u32, u32) {
        let b64 = url.strip_prefix("data:image/png;base64,").unwrap();
        let bytes = base64::engine::general_purpose::STANDARD
            .decode(b64)
            .unwrap();
        let image = image::load_from_memory(&bytes).unwrap();
        (image.width(), image.height())
    }
    #[test]
    fn a_drawn_image_is_kept_as_a_png_with_a_small_copy() {
        let k = Kept::of(pixels(400, 200, 9), 5).unwrap();
        assert_eq!((k.width, k.height, k.at_ms), (400, 200, 5));
        assert_eq!(&k.png[1..4], b"PNG");
        let small = small_size(&k.thumb);
        assert_eq!(small, (160, 80));
        assert!(
            Kept::of(
                Drawn {
                    width: 3,
                    height: 3,
                    rgba: vec![0; 4]
                },
                0
            )
            .is_none()
        );
    }

    #[test]
    fn a_small_image_s_copy_is_itself_and_a_wide_one_s_stays_within_its_width() {
        assert_eq!(
            small_size(&Kept::of(pixels(30, 20, 1), 0).unwrap().thumb),
            (30, 20)
        );
        assert_eq!(
            small_size(&Kept::of(pixels(2400, 100, 1), 0).unwrap().thumb),
            (240, 10)
        );
    }

    #[test]
    fn the_same_image_drawn_again_is_kept_once_where_it_was() {
        let mut s = Store::default();
        assert!(s.keep("a", kept(1, 1)));
        assert!(s.keep("a", kept(2, 2)));
        assert!(!s.keep("a", kept(1, 3)));
        assert_eq!(ids(&s, "a"), [2, 1]);
        assert!(s.keep("b", kept(1, 4)), "another session keeps its own");
    }

    #[test]
    fn a_session_keeps_its_newest_images_up_to_the_bound() {
        let mut s = Store::new(2, usize::MAX);
        for i in 1..=4 {
            s.keep("a", kept(i, u64::from(i)));
        }
        assert_eq!(ids(&s, "a"), [4, 3]);
    }

    #[test]
    fn past_the_bytes_the_oldest_image_of_any_session_gives_way() {
        let one = kept(1, 0).bytes();
        let mut s = Store::new(10, one * 2 + one / 2);
        s.keep("a", kept(1, 1));
        s.keep("b", kept(2, 2));
        s.keep("a", kept(3, 3));
        assert_eq!(ids(&s, "a"), [3]);
        assert_eq!(ids(&s, "b"), [2]);
        s.retain(|session| session == "a");
        assert!(s.of("b").is_empty());
        s.keep("a", kept(4, 4));
        assert_eq!(ids(&s, "a"), [4, 3], "the bytes let go of are free again");
    }
}
