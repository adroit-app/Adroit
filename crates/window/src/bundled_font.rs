//! The font adroit draws the console, the prompt and the file tree's Nerd
//! Font glyphs in unless the user picks another, built into the binary and
//! handed to the webview, so it draws whatever fonts the user has.

use dioxus::desktop::use_asset_handler;
use dioxus::desktop::wry::http::{Response, StatusCode, header};

use crate::fonts::FAMILY;

const PREFIX: &str = "adroit-font";

struct Style {
    file: &'static str,
    weight: u16,
    italic: bool,
    data: &'static [u8],
}

// Only the regular and bold weights, upright and italic, are built in: each
// carries every Nerd Font glyph and costs a megabyte, and they are what the
// console draws ordinary and bold text in.
const STYLES: [Style; 4] = [
    Style {
        file: "JetBrainsMonoNerdFontMono-Regular.woff2",
        weight: 400,
        italic: false,
        data: include_bytes!("../../../assets/fonts/JetBrainsMonoNerdFontMono-Regular.woff2"),
    },
    Style {
        file: "JetBrainsMonoNerdFontMono-Bold.woff2",
        weight: 700,
        italic: false,
        data: include_bytes!("../../../assets/fonts/JetBrainsMonoNerdFontMono-Bold.woff2"),
    },
    Style {
        file: "JetBrainsMonoNerdFontMono-Italic.woff2",
        weight: 400,
        italic: true,
        data: include_bytes!("../../../assets/fonts/JetBrainsMonoNerdFontMono-Italic.woff2"),
    },
    Style {
        file: "JetBrainsMonoNerdFontMono-BoldItalic.woff2",
        weight: 700,
        italic: true,
        data: include_bytes!("../../../assets/fonts/JetBrainsMonoNerdFontMono-BoldItalic.woff2"),
    },
];

/// The stylesheet's `@font-face` rules naming the font [`FAMILY`].
pub fn faces() -> String {
    STYLES
        .iter()
        .map(|s| {
            format!(
                "@font-face {{ font-family: \"{FAMILY}\"; src: url(\"/{PREFIX}/{file}\") format(\"woff2\"); font-weight: {weight}; font-style: {style}; font-display: block; }}",
                file = s.file,
                weight = s.weight,
                style = if s.italic { "italic" } else { "normal" },
            )
        })
        .collect()
}

/// Answer the webview's requests for the files named in [`faces`].
pub fn use_bundled_font() {
    use_asset_handler(PREFIX, |request, responder| {
        responder.respond(answer(request.uri().path()));
    });
}

fn answer(path: &str) -> Response<Vec<u8>> {
    let mut r = Response::new(Vec::new());
    let file = path
        .trim_start_matches('/')
        .strip_prefix(PREFIX)
        .and_then(|p| p.strip_prefix('/'));
    let Some(style) = STYLES.iter().find(|s| Some(s.file) == file) else {
        *r.status_mut() = StatusCode::NOT_FOUND;
        return r;
    };
    *r.body_mut() = style.data.to_vec();
    if let Ok(v) = "font/woff2".parse() {
        r.headers_mut().insert(header::CONTENT_TYPE, v);
    }
    r
}

#[cfg(test)]
mod tests {
    use super::*;

    fn urls() -> Vec<String> {
        let faces = faces();
        faces
            .match_indices("url(\"")
            .map(|(i, _)| {
                let start = i + 5;
                faces[start..start + faces[start..].find('"').unwrap()].to_string()
            })
            .collect()
    }

    #[test]
    fn every_face_s_url_is_answered_with_its_font() {
        let urls = urls();
        assert_eq!(urls.len(), STYLES.len());
        for (url, style) in urls.iter().zip(&STYLES) {
            let r = answer(url);
            assert_eq!(r.status(), StatusCode::OK, "{url}");
            assert_eq!(r.body().as_slice(), style.data, "{url}");
            assert_eq!(r.headers()[header::CONTENT_TYPE], "font/woff2");
        }
    }

    #[test]
    fn the_faces_urls_fall_under_the_handler_s_prefix() {
        for url in urls() {
            assert!(url.starts_with(&format!("/{PREFIX}/")), "{url}");
        }
    }

    #[test]
    fn other_paths_are_not_found() {
        assert_eq!(
            answer("/adroit-font/other.woff2").status(),
            StatusCode::NOT_FOUND
        );
        assert_eq!(
            answer("/elsewhere/JetBrainsMonoNerdFontMono-Regular.woff2").status(),
            StatusCode::NOT_FOUND
        );
    }

    #[test]
    fn every_built_in_style_is_the_family_it_is_named_as_in_its_weight_and_slant() {
        for style in &STYLES {
            let ttf = wuff::decompress_woff2(style.data).unwrap();
            let face = ttf_parser::Face::parse(&ttf, 0).unwrap();
            let family = face
                .names()
                .into_iter()
                .filter(|n| {
                    n.name_id == ttf_parser::name_id::TYPOGRAPHIC_FAMILY
                        || n.name_id == ttf_parser::name_id::FAMILY
                })
                .filter_map(|n| n.to_string())
                .collect::<Vec<_>>();
            assert!(
                family.iter().any(|f| f == FAMILY),
                "{}: {family:?}",
                style.file
            );
            assert_eq!(face.weight().to_number(), style.weight, "{}", style.file);
            assert_eq!(face.is_italic(), style.italic, "{}", style.file);
            assert!(face.is_monospaced(), "{}", style.file);
        }
    }

    #[test]
    fn the_file_tree_draws_its_glyphs_in_the_font() {
        let nf = crate::style::CSS
            .lines()
            .find(|l| l.starts_with(".nf {"))
            .unwrap();
        assert!(nf.contains(&format!("font-family: \"{FAMILY}\", ")), "{nf}");
    }
}
