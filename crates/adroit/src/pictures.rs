//! The images the overview lists: those a session's console drew, kept after
//! they scroll away, and the image files its conversation came across, each
//! with a small copy to show in the list.

use std::collections::HashMap;
use std::hash::{Hash, Hasher};
use std::path::Path;
use std::sync::Arc;
use std::time::SystemTime;

use adroit_claude::links::ImageFile;
use adroit_term::Drawn;
use base64::Engine;
use image::{DynamicImage, ImageFormat, RgbaImage};

use crate::details::Shot;

/// The most images kept for one session: its newest.
const PER_SESSION: usize = 30;

/// The most bytes the kept images take, all sessions together; past it the
/// oldest give way.
const BYTES: usize = 128 * 1024 * 1024;

/// How tall a small copy is, in pixels: twice the height the list draws it
/// at, for a Retina display.
const THUMB_HEIGHT: u32 = 80;

/// How wide a small copy may be, so a wide image stays a strip.
const THUMB_WIDTH: u32 = 240;

/// The largest file shown as it is; a larger one is shown made smaller.
const WHOLE_FILE: u64 = 24 * 1024 * 1024;

/// The longest side of an image made smaller to be shown.
const SHOWN_SIDE: u32 = 4096;

/// The largest file read to make a small copy of.
const READ_FILE: u64 = 128 * 1024 * 1024;

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

/// When a file was last changed, and how long it is, which tell a small copy
/// made of it is still true.
pub type Stamp = (Option<SystemTime>, u64);

/// The stamp of the file at `path`; `None` when it is not a file.
pub fn stamp(path: &Path) -> Option<Stamp> {
    let meta = std::fs::metadata(path).ok().filter(|m| m.is_file())?;
    Some((meta.modified().ok(), meta.len()))
}

/// The small copies of image files made so far, by path.
pub type Thumbs = HashMap<String, (Stamp, Option<Arc<str>>)>;

/// What the overview's images are made from: the console's images kept,
/// and the small copies of image files made.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Gallery {
    pub store: Store,
    pub thumbs: Thumbs,
}

/// The overview's images for `session`: what its console drew, kept in
/// `store`, and the image `files` there are small copies of in `thumbs`,
/// newest first.
pub fn shots(store: &Store, session: &str, files: &[ImageFile], thumbs: &Thumbs) -> Vec<Shot> {
    let drawn = store
        .of(session)
        .iter()
        .map(|k| Shot::console(k.id, k.width, k.height, k.at_ms, Some(k.thumb.clone())));
    let files = files.iter().filter_map(|f| {
        let (_, thumb) = thumbs.get(&f.path)?;
        Some(Shot::file(&f.path, f.touch, f.at_ms, thumb.clone()))
    });
    crate::details::newest_first(drawn.chain(files).collect())
}

/// Bring `thumbs` up to date with `files`: a small copy is made of each
/// file new or changed, and a file gone is taken out of both.
pub fn refresh_thumbs(files: &mut Vec<ImageFile>, thumbs: &mut Thumbs) {
    files.retain(|f| {
        let Some(now) = stamp(Path::new(&f.path)) else {
            thumbs.remove(&f.path);
            return false;
        };
        if thumbs.get(&f.path).is_none_or(|(was, _)| *was != now) {
            thumbs.insert(f.path.clone(), (now, file_thumb(Path::new(&f.path))));
        }
        true
    });
    if thumbs.len() > files.len() * 2 + 64 {
        thumbs.retain(|p, _| files.iter().any(|f| f.path == *p));
    }
}

fn is_svg(path: &Path) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| e.eq_ignore_ascii_case("svg"))
}

/// The image at `path`, decoded; an SVG is drawn with its longest side at
/// most `side` pixels.
fn decode(path: &Path, bytes: &[u8], side: u32) -> Option<DynamicImage> {
    if !is_svg(path) {
        return image::load_from_memory(bytes).ok();
    }
    use resvg::{tiny_skia, usvg};
    let tree = usvg::Tree::from_data(bytes, &usvg::Options::default()).ok()?;
    let size = tree.size();
    let scale = (side as f32 / size.width().max(size.height())).min(4.0);
    let (w, h) = (
        (size.width() * scale).ceil() as u32,
        (size.height() * scale).ceil() as u32,
    );
    let mut pixmap = tiny_skia::Pixmap::new(w.max(1), h.max(1))?;
    resvg::render(
        &tree,
        tiny_skia::Transform::from_scale(scale, scale),
        &mut pixmap.as_mut(),
    );
    // tiny-skia's pixels are premultiplied; an image's are not.
    let rgba = pixmap
        .pixels()
        .iter()
        .flat_map(|p| {
            let c = p.demultiply();
            [c.red(), c.green(), c.blue(), c.alpha()]
        })
        .collect();
    Some(DynamicImage::ImageRgba8(RgbaImage::from_raw(
        pixmap.width(),
        pixmap.height(),
        rgba,
    )?))
}

/// A small copy of the image file at `path`. One that cannot be decoded
/// here, as HEIC, is left to the window to show when it is small enough.
pub fn file_thumb(path: &Path) -> Option<Arc<str>> {
    let len = std::fs::metadata(path).ok()?.len();
    if len > READ_FILE {
        return None;
    }
    let bytes = std::fs::read(path).ok()?;
    match decode(path, &bytes, THUMB_WIDTH) {
        Some(image) => thumb(&image),
        None if len <= 1024 * 1024 => Some(data_url(mime(path), &bytes)),
        None => None,
    }
}

/// The image file at `path`, to show whole: as it is, or, when it is large,
/// made smaller.
pub fn file_shown(path: &Path) -> Option<Arc<str>> {
    let len = std::fs::metadata(path).ok()?.len();
    if len > READ_FILE {
        return None;
    }
    let bytes = std::fs::read(path).ok()?;
    if len <= WHOLE_FILE {
        return Some(data_url(mime(path), &bytes));
    }
    let image = decode(path, &bytes, SHOWN_SIDE)?;
    let image = if image.width().max(image.height()) > SHOWN_SIDE {
        image.thumbnail(SHOWN_SIDE, SHOWN_SIDE)
    } else {
        image
    };
    Some(data_url("image/png", &png(&image)?))
}

/// A kept image, to show whole.
pub fn kept_shown(kept: &Kept) -> Arc<str> {
    data_url("image/png", &kept.png)
}

/// `image` within the small copy's bounds, as a PNG `data:` URL.
fn thumb(image: &DynamicImage) -> Option<Arc<str>> {
    let small = if image.height() > THUMB_HEIGHT || image.width() > THUMB_WIDTH {
        image.thumbnail(THUMB_WIDTH, THUMB_HEIGHT)
    } else {
        image.clone()
    };
    Some(data_url("image/png", &png(&small)?))
}

fn png(image: &DynamicImage) -> Option<Vec<u8>> {
    let mut out = std::io::Cursor::new(Vec::new());
    image.write_to(&mut out, ImageFormat::Png).ok()?;
    Some(out.into_inner())
}

fn data_url(mime: &str, bytes: &[u8]) -> Arc<str> {
    let b64 = base64::engine::general_purpose::STANDARD.encode(bytes);
    format!("data:{mime};base64,{b64}").into()
}

/// The media type of the image file at `path`, by its extension.
fn mime(path: &Path) -> &'static str {
    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase();
    match ext.as_str() {
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "gif" => "image/gif",
        "webp" => "image/webp",
        "bmp" => "image/bmp",
        "svg" => "image/svg+xml",
        "heic" => "image/heic",
        "heif" => "image/heif",
        "tif" | "tiff" => "image/tiff",
        "ico" => "image/x-icon",
        "avif" => "image/avif",
        _ => "application/octet-stream",
    }
}

#[cfg(test)]
mod tests {
    use adroit_claude::links::Touch;

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

    fn small_size(url: &str) -> (u32, u32) {
        let b64 = url.strip_prefix("data:image/png;base64,").unwrap();
        let bytes = base64::engine::general_purpose::STANDARD
            .decode(b64)
            .unwrap();
        let image = image::load_from_memory(&bytes).unwrap();
        (image.width(), image.height())
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

    fn scratch(name: &str) -> std::path::PathBuf {
        let dir =
            std::env::temp_dir().join(format!("adroit-pictures-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn file(path: &Path, at_ms: Option<u64>) -> ImageFile {
        ImageFile {
            path: path.to_string_lossy().into_owned(),
            touch: Touch::Read,
            at_ms,
        }
    }

    #[test]
    fn an_image_file_gets_a_small_copy_and_one_gone_is_dropped() {
        let dir = scratch("files");
        let png = dir.join("a.png");
        DynamicImage::ImageRgba8(RgbaImage::new(300, 300))
            .save(&png)
            .unwrap();
        let svg = dir.join("b.svg");
        std::fs::write(
            &svg,
            r#"<svg xmlns="http://www.w3.org/2000/svg" width="10" height="20"><rect width="10" height="20"/></svg>"#,
        )
        .unwrap();
        let mut files = vec![
            file(&png, Some(2)),
            file(&dir.join("gone.png"), Some(3)),
            file(&svg, Some(1)),
        ];
        let mut thumbs = Thumbs::new();
        refresh_thumbs(&mut files, &mut thumbs);
        assert_eq!(files.len(), 2);
        let small = |p: &Path| small_size(thumbs[&*p.to_string_lossy()].1.as_deref().unwrap());
        assert_eq!(small(&png), (80, 80));
        assert_eq!(small(&svg), (40, 80));
        let got = shots(&Store::default(), "s", &files, &thumbs);
        let keys: Vec<_> = got.iter().map(|s| s.key.clone()).collect();
        assert_eq!(
            keys,
            [
                png.to_string_lossy().into_owned(),
                svg.to_string_lossy().into_owned()
            ]
        );
    }

    #[test]
    fn a_file_not_decoded_here_is_shown_as_it_is_with_its_type() {
        let dir = scratch("raw");
        let heic = dir.join("p.HEIC");
        std::fs::write(&heic, b"not decoded here").unwrap();
        let url = file_thumb(&heic).unwrap();
        assert!(url.starts_with("data:image/heic;base64,"), "{url}");
        assert!(
            file_shown(&heic)
                .unwrap()
                .starts_with("data:image/heic;base64,")
        );
    }

    #[test]
    fn the_console_s_images_and_the_files_run_newest_first_together() {
        let mut store = Store::default();
        store.keep("s", kept(1, 5));
        let dir = scratch("together");
        let png = dir.join("a.png");
        DynamicImage::ImageRgba8(RgbaImage::new(4, 4))
            .save(&png)
            .unwrap();
        let mut files = vec![
            file(&png, Some(9)),
            file(&png.with_file_name("b.png"), None),
        ];
        let mut thumbs = Thumbs::new();
        refresh_thumbs(&mut files, &mut thumbs);
        let got = shots(&store, "s", &files, &thumbs);
        assert_eq!(got.len(), 2);
        assert_eq!(got[0].at_ms, Some(9));
        assert_eq!(got[1].at_ms, Some(5));
    }
}
