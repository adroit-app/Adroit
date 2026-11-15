//! The Markdown viewer's document: the file as Arto renders it, drawn in the
//! viewer's frame by Arto's frontend.

use std::collections::BTreeMap;
use std::path::{Component, Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::SystemTime;

use adroit_core::settings::ThemeKind;
use arto_frontend::{Highlighting, Libraries};
use arto_markdown::{ImageResolution, RenderOptions};
use dioxus::desktop::use_asset_handler;
use dioxus::desktop::wry::http::{Response, StatusCode, header};

use super::serve::{self, ORIGIN};

/// Where Arto's stylesheet and scripts are served, for every viewer alike.
pub const ARTO_PREFIX: &str = "adroit-arto";

/// The largest document the viewer renders. The whole of it is rendered
/// again on every change, and a larger one is rarely meant to be read.
pub const MOST_BYTES: u64 = 8 * 1024 * 1024;

/// The script the document runs beside Arto's, which hands the viewer's
/// keys, scrolls and zoom between the frame and the window.
const DOCUMENT_SCRIPT: &str = include_str!("../../assets/js/markdown-page.js");

/// Why a document is not shown.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Unshown {
    Unreadable,
    TooLarge,
    Unrenderable,
}

impl Unshown {
    /// What the viewer says in the document's place.
    pub fn says(self) -> &'static str {
        match self {
            Unshown::Unreadable => "This file cannot be read",
            Unshown::TooLarge => "This file is too large to preview",
            Unshown::Unrenderable => "This document cannot be rendered",
        }
    }
}

/// A local image a document shows, which its viewer serves by id.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Image {
    pub path: PathBuf,
    pub mime: &'static str,
}

/// The images a viewer serves, by the id its last render gave each.
pub type Images = Arc<Mutex<BTreeMap<String, Image>>>;

/// A document as rendered: the fragment for the viewer's article, the
/// bundles it needs, and the images it shows.
#[derive(Debug, Clone, PartialEq)]
pub struct Rendered {
    pub html: String,
    pub libraries: Libraries,
    pub images: BTreeMap<String, Image>,
}

/// The file at `path` rendered as Arto renders it, its images asked for
/// under `image_base` and kept to those under `root`.
pub fn render(
    path: impl AsRef<Path>,
    root: impl AsRef<Path>,
    image_base: impl Into<String>,
) -> Result<Rendered, Unshown> {
    let path = path.as_ref();
    let len = std::fs::metadata(path)
        .map_err(|_| Unshown::Unreadable)?
        .len();
    if len > MOST_BYTES {
        return Err(Unshown::TooLarge);
    }
    let bytes = std::fs::read(path).map_err(|_| Unshown::Unreadable)?;
    let text = String::from_utf8_lossy(&bytes);
    let options = RenderOptions {
        images: ImageResolution::Deferred {
            base_url: image_base.into(),
        },
        ..RenderOptions::default()
    };
    let result = arto_markdown::render_to_html_with_toc(text.as_ref(), path, &options)
        .map_err(|_| Unshown::Unrenderable)?;
    // A document may name any file as an image, absolute paths and those
    // outside its folder included; the viewer reads only what lies under
    // the worktree the document is in.
    let root = root.as_ref().canonicalize().ok();
    let images = result
        .images
        .into_iter()
        .filter(|i| root.as_ref().is_some_and(|r| i.path.starts_with(r)))
        .map(|i| {
            (
                i.id,
                Image {
                    path: i.path,
                    mime: i.mime,
                },
            )
        })
        .collect();
    let libraries = Libraries::for_fragment(&result.html);
    Ok(Rendered {
        html: result.html,
        libraries,
        images,
    })
}

/// The URL of Arto's file `name`.
fn arto_url(name: &str) -> String {
    format!("{ORIGIN}{ARTO_PREFIX}/{name}")
}

/// The scripts `libraries` names, in the order they are to load.
pub fn library_names(libraries: Libraries) -> Vec<&'static str> {
    let highlighter = match libraries.highlighting {
        Highlighting::None => None,
        Highlighting::Common => Some("hljs-common.js"),
        Highlighting::Every => Some("hljs-all.js"),
    };
    [
        libraries.mermaid.then_some("mermaid.js"),
        libraries.math.then_some("math.js"),
        highlighter,
    ]
    .into_iter()
    .flatten()
    .collect()
}

/// The theme of Arto's that a theme of `kind` is drawn beside.
pub fn arto_theme(kind: ThemeKind) -> &'static str {
    match kind {
        ThemeKind::Light => "light",
        ThemeKind::Dark => "dark",
    }
}

/// The document the viewer's frame is given: `fragment` in Arto's viewer,
/// painted in `theme`, after Arto's stylesheet, its runtime and the bundles
/// `libraries` names.
pub fn markdown_document(fragment: &str, libraries: Libraries, theme: &str) -> String {
    let scripts: String = std::iter::once("embed.js")
        .chain(library_names(libraries))
        .map(|name| format!("<script src=\"{}\"></script>", arto_url(name)))
        .collect();
    format!(
        "<!doctype html><html data-adroit-theme=\"{theme}\"><head><meta charset=\"utf-8\">\
         <link rel=\"stylesheet\" href=\"{css}\">{page}{scripts}<script>{own}</script></head>\
         <body><div class=\"markdown-viewer\"><article class=\"markdown-body\">{fragment}</article></div></body></html>",
        css = arto_url("embed.css"),
        page = serve::PAGE_SCRIPT,
        own = DOCUMENT_SCRIPT,
    )
}

/// The message that puts `rendered` in place of what the frame shows, with
/// the bundles it needs that the frame may not have loaded yet.
pub fn update_message(rendered: &Rendered) -> String {
    let scripts: Vec<String> = library_names(rendered.libraries)
        .into_iter()
        .map(arto_url)
        .collect();
    serde_json::json!({ "markdown": rendered.html, "scripts": scripts }).to_string()
}

/// The message that paints the frame's document in `theme`.
pub fn theme_message(theme: &str) -> String {
    serde_json::json!({ "theme": theme }).to_string()
}

/// What tells one state of the file at `path` from the next: its length and
/// when it was last written; none while it cannot be read.
pub fn stamp(path: impl AsRef<Path>) -> Option<(u64, SystemTime)> {
    let meta = std::fs::metadata(path).ok()?;
    Some((meta.len(), meta.modified().ok()?))
}

/// The file a link the document at `document` writes as `written` leads to:
/// relative to the document's folder, without the place within it. None for
/// a link to the document's own places, or to an address.
pub fn link_target(document: impl AsRef<Path>, written: impl AsRef<str>) -> Option<PathBuf> {
    let written = written.as_ref();
    let file = written.split(['#', '?']).next().unwrap_or_default();
    if file.is_empty() || file.contains("://") {
        return None;
    }
    let file = serve::decode(file).unwrap_or_else(|| file.to_string());
    let folder = document.as_ref().parent()?;
    let mut out = PathBuf::new();
    for part in folder.join(file).components() {
        match part {
            Component::ParentDir => {
                out.pop();
            }
            Component::CurDir => {}
            part => out.push(part),
        }
    }
    Some(out)
}

/// Arto's file `rel`, as served under [`ARTO_PREFIX`], with its media type.
pub fn arto_asset(rel: &str) -> Option<(&'static str, &'static str)> {
    const JS: &str = "text/javascript; charset=utf-8";
    Some(match rel {
        "embed.css" => (arto_frontend::STYLESHEET, "text/css; charset=utf-8"),
        "embed.js" => (arto_frontend::RUNTIME, JS),
        "mermaid.js" => (arto_frontend::MERMAID, JS),
        "math.js" => (arto_frontend::MATH, JS),
        "hljs-common.js" => (arto_frontend::HLJS_COMMON, JS),
        "hljs-all.js" => (arto_frontend::HLJS_ALL, JS),
        _ => return None,
    })
}

/// Answer the webview's requests for Arto's files, for every viewer the
/// window draws.
pub fn use_arto_assets() {
    use_asset_handler(ARTO_PREFIX, |request, responder| {
        let rel = request
            .uri()
            .path()
            .trim_start_matches('/')
            .strip_prefix(ARTO_PREFIX)
            .unwrap_or_default()
            .trim_start_matches('/')
            .to_string();
        responder.respond(arto_response(&rel));
    });
}

fn arto_response(rel: &str) -> Response<Vec<u8>> {
    let mut r = Response::new(Vec::new());
    let Some((body, media)) = arto_asset(rel) else {
        *r.status_mut() = StatusCode::NOT_FOUND;
        return r;
    };
    *r.body_mut() = body.as_bytes().to_vec();
    if let Ok(v) = media.parse() {
        r.headers_mut().insert(header::CONTENT_TYPE, v);
    }
    // The document runs in a frame with an origin of its own, and asks for
    // these as from another origin.
    if let Ok(v) = "*".parse() {
        r.headers_mut()
            .insert(header::ACCESS_CONTROL_ALLOW_ORIGIN, v);
    }
    r
}

/// The id of the image `rel` asks for, `<id>` or `<id>.<extension>` as the
/// render wrote it.
pub fn image_id(rel: &str) -> &str {
    rel.split(['.', '?', '#']).next().unwrap_or_default()
}

/// The answer to a request for `image`, one the last render handed the
/// viewer, or for none.
pub fn image_response(image: Option<&Image>) -> Response<Vec<u8>> {
    let found = image.and_then(|image| {
        let len = std::fs::metadata(&image.path).ok()?.len();
        (len <= arto_markdown::MAX_IMAGE_BYTES)
            .then(|| std::fs::read(&image.path).ok())
            .flatten()
            .map(|bytes| (bytes, image.mime))
    });
    let mut r = Response::new(Vec::new());
    match found {
        Some((bytes, mime)) => {
            *r.body_mut() = bytes;
            if let Ok(v) = mime.parse() {
                r.headers_mut().insert(header::CONTENT_TYPE, v);
            }
        }
        None => *r.status_mut() = StatusCode::NOT_FOUND,
    }
    r
}

#[cfg(test)]
mod tests {
    use super::*;

    fn every() -> Libraries {
        Libraries {
            mermaid: true,
            math: true,
            highlighting: Highlighting::Every,
        }
    }

    fn none() -> Libraries {
        Libraries::for_fragment("<p>hi</p>")
    }

    #[test]
    fn the_document_places_the_fragment_in_arto_s_viewer() {
        let doc = markdown_document("<p>hi</p>", none(), "light");
        assert!(doc.contains(
            "<div class=\"markdown-viewer\"><article class=\"markdown-body\"><p>hi</p></article></div>"
        ));
        assert!(doc.contains(&format!("href=\"{ORIGIN}{ARTO_PREFIX}/embed.css\"")));
    }

    #[test]
    fn the_runtime_loads_before_the_bundles_the_fragment_needs_and_no_others() {
        let doc = markdown_document("<p>hi</p>", every(), "light");
        let at = |name: &str| doc.find(&format!("{ARTO_PREFIX}/{name}")).unwrap();
        assert!(at("embed.js") < at("mermaid.js"));
        assert!(at("mermaid.js") < at("math.js"));
        assert!(at("math.js") < at("hljs-all.js"));
        let prose = markdown_document("<p>hi</p>", none(), "light");
        for name in ["mermaid.js", "math.js", "hljs-common.js", "hljs-all.js"] {
            assert!(!prose.contains(name), "{name}");
        }
    }

    #[test]
    fn every_script_the_document_names_is_served_and_nothing_else_is() {
        for name in std::iter::once("embed.js").chain(library_names(every())) {
            assert!(arto_asset(name).is_some(), "{name}");
        }
        assert!(arto_asset("hljs-common.js").is_some());
        assert!(arto_asset("embed.css").is_some());
        assert!(arto_asset("../etc/passwd").is_none());
        assert_eq!(arto_response("nope").status(), StatusCode::NOT_FOUND);
    }

    #[test]
    fn a_document_is_rendered_with_its_headings_reachable_and_its_libraries_named() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("a.md");
        std::fs::write(&path, "# Title\n\n```mermaid\ngraph TD;\n```\n").unwrap();
        let r = render(&path, dir.path(), "x").unwrap();
        assert!(r.html.contains("id=\"title\""), "{}", r.html);
        assert!(r.libraries.mermaid);
    }

    #[test]
    fn a_file_that_is_not_utf_8_is_rendered_all_the_same() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("a.md");
        std::fs::write(&path, b"caf\xe9\n").unwrap();
        assert!(render(&path, dir.path(), "x").unwrap().html.contains("caf"));
    }

    #[test]
    fn a_missing_or_too_large_file_says_why_it_is_not_shown() {
        let dir = tempfile::tempdir().unwrap();
        assert_eq!(
            render(dir.path().join("none.md"), dir.path(), "x"),
            Err(Unshown::Unreadable)
        );
        let big = dir.path().join("big.md");
        std::fs::File::create(&big)
            .unwrap()
            .set_len(MOST_BYTES + 1)
            .unwrap();
        assert_eq!(render(&big, dir.path(), "x"), Err(Unshown::TooLarge));
    }

    #[test]
    fn only_the_images_under_the_root_are_kept_to_be_served() {
        let outer = tempfile::tempdir().unwrap();
        let root = outer.path().join("w");
        std::fs::create_dir(&root).unwrap();
        std::fs::write(root.join("in.png"), b"png").unwrap();
        std::fs::write(outer.path().join("out.png"), b"png").unwrap();
        let path = root.join("a.md");
        std::fs::write(&path, "![a](in.png) ![b](../out.png)\n").unwrap();
        let r = render(&path, &root, "dioxus://index.html/v/img").unwrap();
        let kept: Vec<_> = r.images.values().map(|i| i.path.clone()).collect();
        assert_eq!(kept, [root.join("in.png").canonicalize().unwrap()]);
        let (id, _) = r.images.iter().next().unwrap();
        assert!(
            r.html
                .contains(&format!("dioxus://index.html/v/img/{id}.png"))
        );
    }

    #[test]
    fn an_image_is_served_by_its_id_with_or_without_its_extension() {
        let dir = tempfile::tempdir().unwrap();
        let png = dir.path().join("a.png");
        std::fs::write(&png, b"png").unwrap();
        let images = BTreeMap::from([(
            "abc".to_string(),
            Image {
                path: png,
                mime: "image/png",
            },
        )]);
        for rel in ["abc", "abc.png"] {
            let r = image_response(images.get(image_id(rel)));
            assert_eq!(r.status(), StatusCode::OK, "{rel}");
            assert_eq!(r.body(), b"png");
            assert_eq!(r.headers()[header::CONTENT_TYPE], "image/png");
        }
        assert_eq!(
            image_response(images.get(image_id("other.png"))).status(),
            StatusCode::NOT_FOUND
        );
    }

    #[test]
    fn the_document_is_painted_in_the_theme_of_the_kind_in_force() {
        assert_eq!(arto_theme(ThemeKind::Light), "light");
        assert_eq!(arto_theme(ThemeKind::Dark), "dark");
        let doc = markdown_document("<p>hi</p>", none(), "dark");
        assert!(doc.contains("data-adroit-theme=\"dark\""));
        let message: serde_json::Value = serde_json::from_str(&theme_message("dark")).unwrap();
        assert_eq!(message["theme"], "dark");
    }

    #[test]
    fn a_document_rendered_again_reaches_the_frame_with_the_bundles_it_needs() {
        let rendered = Rendered {
            html: "<p>\"new\"</p>".into(),
            libraries: Libraries {
                mermaid: true,
                ..none()
            },
            images: BTreeMap::new(),
        };
        let message: serde_json::Value = serde_json::from_str(&update_message(&rendered)).unwrap();
        assert_eq!(message["markdown"], "<p>\"new\"</p>");
        assert_eq!(
            message["scripts"],
            serde_json::json!([format!("{ORIGIN}{ARTO_PREFIX}/mermaid.js")])
        );
    }

    #[test]
    fn a_file_written_again_is_told_apart_and_one_gone_has_no_stamp() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("a.md");
        std::fs::write(&path, "a").unwrap();
        let before = stamp(&path);
        std::fs::write(&path, "ab").unwrap();
        assert_ne!(stamp(&path), before);
        std::fs::remove_file(&path).unwrap();
        assert_eq!(stamp(&path), None);
    }

    #[test]
    fn a_link_leads_to_a_file_beside_the_document_without_the_place_in_it() {
        let doc = "/w/docs/guide/a.md";
        for (written, file) in [
            ("b.md", "/w/docs/guide/b.md"),
            ("./b.md#setup", "/w/docs/guide/b.md"),
            ("../api/c.md", "/w/docs/api/c.md"),
            ("../../README.md?plain=1", "/w/README.md"),
            ("my%20notes.md", "/w/docs/guide/my notes.md"),
            ("/etc/hosts", "/etc/hosts"),
        ] {
            assert_eq!(link_target(doc, written), Some(file.into()), "{written}");
        }
        for written in ["#setup", "", "https://example.com/a.md"] {
            assert_eq!(link_target(doc, written), None, "{written}");
        }
    }
}
