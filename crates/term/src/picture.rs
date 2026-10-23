//! Images as the window shows them: encoded as PNG, as `data:` URLs, and the
//! small copy a list draws.

use std::sync::Arc;

use base64::Engine;
use image::{DynamicImage, ImageFormat};

/// How tall a small copy is, in pixels: twice the height the list draws it
/// at, for a Retina display.
pub const THUMB_HEIGHT: u32 = 80;

/// How wide a small copy may be, so a wide image stays a strip.
pub const THUMB_WIDTH: u32 = 240;

/// `image` within the small copy's bounds, as a PNG `data:` URL.
pub fn thumb(image: &DynamicImage) -> Option<Arc<str>> {
    let small = if image.height() > THUMB_HEIGHT || image.width() > THUMB_WIDTH {
        image.thumbnail(THUMB_WIDTH, THUMB_HEIGHT)
    } else {
        image.clone()
    };
    Some(data_url("image/png", &png(&small)?))
}

/// `image` encoded as PNG.
pub fn png(image: &DynamicImage) -> Option<Vec<u8>> {
    let mut out = std::io::Cursor::new(Vec::new());
    image.write_to(&mut out, ImageFormat::Png).ok()?;
    Some(out.into_inner())
}

/// `bytes` of `mime` as a `data:` URL.
pub fn data_url(mime: &str, bytes: &[u8]) -> Arc<str> {
    let b64 = base64::engine::general_purpose::STANDARD.encode(bytes);
    format!("data:{mime};base64,{b64}").into()
}
