//! The image files a session's conversation came across, each with a small
//! copy for the overview to list, and shown whole when asked.

use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;
use std::time::SystemTime;

use adroit_agents::claude::links::ImageFile;
use adroit_term::picture::{THUMB_WIDTH, data_url, png, thumb};
use image::{DynamicImage, RgbaImage};

/// The largest file shown as it is; a larger one is shown made smaller.
const WHOLE_FILE: u64 = 24 * 1024 * 1024;

/// The longest side of an image made smaller to be shown.
const SHOWN_SIDE: u32 = 4096;

/// The largest file read to make a small copy of.
const READ_FILE: u64 = 128 * 1024 * 1024;

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
    use adroit_agents::claude::links::Touch;
    use base64::Engine;

    use super::*;

    fn small_size(url: &str) -> (u32, u32) {
        let b64 = url.strip_prefix("data:image/png;base64,").unwrap();
        let bytes = base64::engine::general_purpose::STANDARD
            .decode(b64)
            .unwrap();
        let image = image::load_from_memory(&bytes).unwrap();
        (image.width(), image.height())
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
        let keys: Vec<_> = files.iter().map(|f| f.path.clone()).collect();
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
}
