//! The font the file tree's Nerd Font glyphs are drawn in, built into the
//! binary and handed to the webview, so the glyphs draw whatever font the
//! user has.

use dioxus::desktop::use_asset_handler;
use dioxus::desktop::wry::http::{Response, StatusCode, header};

const FONT: &[u8] = include_bytes!("../../../assets/fonts/SymbolsNerdFontMono-Regular.ttf");

const PREFIX: &str = "adroit-symbols-font";

/// The stylesheet's `@font-face` naming the font "Adroit Symbols".
pub const FACE: &str = concat!(
    "@font-face { font-family: \"Adroit Symbols\"; ",
    "src: url(\"/adroit-symbols-font/SymbolsNerdFontMono-Regular.ttf\") format(\"truetype\"); ",
    "font-display: block; }"
);

/// Answer the webview's requests for the font named in [`FACE`].
pub fn use_symbols_font() {
    use_asset_handler(PREFIX, |request, responder| {
        responder.respond(answer(request.uri().path()));
    });
}

fn answer(path: &str) -> Response<Vec<u8>> {
    let mut r = Response::new(Vec::new());
    if path.trim_start_matches('/') != "adroit-symbols-font/SymbolsNerdFontMono-Regular.ttf" {
        *r.status_mut() = StatusCode::NOT_FOUND;
        return r;
    }
    *r.body_mut() = FONT.to_vec();
    if let Ok(v) = "font/ttf".parse() {
        r.headers_mut().insert(header::CONTENT_TYPE, v);
    }
    r
}

#[cfg(test)]
mod tests {
    use super::*;

    fn face_url() -> &'static str {
        let start = FACE.find("url(\"").unwrap() + 5;
        let end = start + FACE[start..].find('"').unwrap();
        &FACE[start..end]
    }

    #[test]
    fn the_face_s_url_is_answered_with_the_font() {
        let r = answer(face_url());
        assert_eq!(r.status(), StatusCode::OK);
        assert_eq!(r.body().as_slice(), FONT);
        assert_eq!(r.headers()[header::CONTENT_TYPE], "font/ttf");
    }

    #[test]
    fn the_face_s_url_falls_under_the_handler_s_prefix() {
        assert!(
            face_url().starts_with(&format!("/{PREFIX}/")),
            "{}",
            face_url()
        );
    }

    #[test]
    fn other_paths_are_not_found() {
        assert_eq!(
            answer("/adroit-symbols-font/other.ttf").status(),
            StatusCode::NOT_FOUND
        );
    }
}
