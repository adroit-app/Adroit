//! Hand a viewer's file to the webview, and nothing else on the disk.

use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::{Component, Path, PathBuf};

use dioxus::desktop::wry::http::{Response, StatusCode, header};

use super::kind::media_type;

/// What one viewer may read.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Scope {
    /// The one file.
    File(PathBuf),
    /// The file and everything under its directory, which an HTML page links
    /// to by relative paths.
    Directory { root: PathBuf, entry: PathBuf },
}

impl Scope {
    /// The scope that serves the page at `entry` and what it links to.
    pub fn page(entry: impl Into<PathBuf>) -> Scope {
        let entry = entry.into();
        let root = entry
            .parent()
            .map(Path::to_path_buf)
            .unwrap_or_else(|| PathBuf::from("."));
        Scope::Directory { root, entry }
    }

    /// The file the viewer opened.
    pub fn entry(&self) -> &Path {
        match self {
            Scope::File(p) | Scope::Directory { entry: p, .. } => p,
        }
    }

    /// The path, under the viewer's own prefix, that names the entry.
    pub fn entry_url(&self) -> String {
        encode(
            &self
                .entry()
                .file_name()
                .unwrap_or_default()
                .to_string_lossy(),
        )
    }

    /// The file `rel`, a path under the viewer's own prefix as the webview
    /// asks for it, names; None when it is outside the scope or missing.
    pub fn resolve(&self, rel: &str) -> Option<PathBuf> {
        let rel = decode(rel.split(['?', '#']).next().unwrap_or_default())?;
        let parts = rel
            .split('/')
            .filter(|p| !p.is_empty())
            .map(
                |p| match Path::new(p).components().collect::<Vec<_>>()[..] {
                    [Component::Normal(_)] if !p.contains('\\') => Some(p),
                    _ => None,
                },
            )
            .collect::<Option<Vec<_>>>()?;
        match self {
            Scope::File(path) => {
                let name = path.file_name()?.to_str()?;
                (parts == [name])
                    .then(|| path.clone())
                    .filter(|p| p.is_file())
            }
            Scope::Directory { root, .. } => {
                let root = root.canonicalize().ok()?;
                let found = parts
                    .iter()
                    .fold(root.clone(), |p, part| p.join(part))
                    .canonicalize()
                    .ok()?;
                (found.starts_with(&root) && found.is_file()).then_some(found)
            }
        }
    }
}

/// `text` with every byte a URL path segment cannot hold escaped.
pub fn encode(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for b in text.bytes() {
        if b.is_ascii_alphanumeric() || b"-._~".contains(&b) {
            out.push(b as char);
        } else {
            out.push_str(&format!("%{b:02X}"));
        }
    }
    out
}

/// `text` with its `%XX` escapes undone; None when they do not make UTF-8.
pub fn decode(text: &str) -> Option<String> {
    let bytes = text.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' {
            let hex = std::str::from_utf8(bytes.get(i + 1..i + 3)?).ok()?;
            out.push(u8::from_str_radix(hex, 16).ok()?);
            i += 3;
        } else {
            out.push(bytes[i]);
            i += 1;
        }
    }
    String::from_utf8(out).ok().filter(|s| !s.contains('\0'))
}

/// The most bytes one answer carries: a movie is read a piece at a time, as
/// the webview asks for it, never whole.
const MOST_BYTES: u64 = 8 * 1024 * 1024;

/// The bytes a `Range` header asks of a file `len` bytes long, as an
/// inclusive start and end; None when none of them are in the file.
pub fn byte_range(header: &str, len: u64) -> Option<(u64, u64)> {
    let spec = header.trim().strip_prefix("bytes=")?;
    // Several ranges in one request are answered with the first.
    let spec = spec.split(',').next()?.trim();
    let (start, end) = spec.split_once('-')?;
    let last = len.checked_sub(1)?;
    let (start, end) = match (start.trim(), end.trim()) {
        ("", "") => return None,
        ("", n) => match n.parse::<u64>().ok()? {
            0 => return None,
            n => (len.saturating_sub(n), last),
        },
        (s, "") => (s.parse().ok()?, last),
        (s, e) => (s.parse().ok()?, e.parse::<u64>().ok()?.min(last)),
    };
    if start > end || start >= len {
        return None;
    }
    Some((start, end.min(start + MOST_BYTES - 1)))
}

/// Where the window's own page and everything it serves are, as the webview
/// sees them.
#[cfg(target_os = "windows")]
pub const ORIGIN: &str = "http://dioxus.index.html/";
#[cfg(not(target_os = "windows"))]
pub const ORIGIN: &str = "dioxus://index.html/";

/// A script put into the page the HTML viewer shows, which lets the viewer
/// scroll and zoom the page and hears the keys pressed in it: the page runs
/// apart from the window, and nothing else of the window reaches into it.
pub const PAGE_SCRIPT: &str = concat!(
    "<script>",
    include_str!("../../assets/js/viewer-page.js"),
    "</script>"
);

/// `page` as the HTML viewer hands it to its frame: its relative links
/// resolved under `base`, with [`PAGE_SCRIPT`], both first in its head.
///
/// The frame is given the page itself rather than its address: the window
/// refuses to navigate anywhere once it has loaded, frames included, while
/// what a page loads is not navigation.
pub fn page_document(page: &str, base: &str) -> String {
    let lower = page.to_ascii_lowercase();
    let after = |tag: &str| {
        let at = lower.find(tag)?;
        Some(at + lower[at..].find('>')? + 1)
    };
    let at = after("<head")
        .or_else(|| after("<html"))
        .or_else(|| after("<!doctype"))
        .unwrap_or(0);
    let base = base.replace('&', "&amp;").replace('"', "&quot;");
    let mut out = String::with_capacity(page.len() + PAGE_SCRIPT.len() + 64);
    out.push_str(&page[..at]);
    out.push_str(&format!("<base href=\"{base}\">"));
    out.push_str(PAGE_SCRIPT);
    out.push_str(&page[at..]);
    out
}

fn status(code: StatusCode) -> Response<Vec<u8>> {
    let mut r = Response::new(Vec::new());
    *r.status_mut() = code;
    r
}

/// The answer to a request for `rel` within `scope`, with the `Range`
/// header the webview sent, if any.
pub fn respond(scope: &Scope, rel: &str, range: Option<&str>) -> Response<Vec<u8>> {
    match scope.resolve(rel) {
        Some(path) => file_response(&path, range).unwrap_or_else(|_| status(StatusCode::NOT_FOUND)),
        None => status(StatusCode::NOT_FOUND),
    }
}

fn with_type(mut r: Response<Vec<u8>>, path: &Path) -> Response<Vec<u8>> {
    let h = r.headers_mut();
    if let Ok(v) = media_type(path).parse() {
        h.insert(header::CONTENT_TYPE, v);
    }
    if let Ok(v) = "no-store".parse() {
        h.insert(header::CACHE_CONTROL, v);
    }
    r
}

fn file_response(path: &Path, range: Option<&str>) -> std::io::Result<Response<Vec<u8>>> {
    let mut file = File::open(path)?;
    let len = file.metadata()?.len();
    let (start, end, partial) = match range.map(|h| byte_range(h, len)) {
        None if len <= MOST_BYTES => (0, len.saturating_sub(1), false),
        None => (0, MOST_BYTES - 1, true),
        Some(Some((s, e))) => (s, e, true),
        Some(None) => {
            let mut r = status(StatusCode::RANGE_NOT_SATISFIABLE);
            if let Ok(v) = format!("bytes */{len}").parse() {
                r.headers_mut().insert(header::CONTENT_RANGE, v);
            }
            return Ok(r);
        }
    };
    let mut body = Vec::new();
    if len > 0 {
        file.seek(SeekFrom::Start(start))?;
        file.take(end - start + 1).read_to_end(&mut body)?;
    }
    let mut r = with_type(Response::new(body), path);
    let h = r.headers_mut();
    if let Ok(v) = "bytes".parse() {
        h.insert(header::ACCEPT_RANGES, v);
    }
    if partial {
        *r.status_mut() = StatusCode::PARTIAL_CONTENT;
        if let Ok(v) = format!("bytes {start}-{end}/{len}").parse() {
            r.headers_mut().insert(header::CONTENT_RANGE, v);
        }
    }
    Ok(r)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tree() -> tempfile::TempDir {
        let dir = tempfile::tempdir().unwrap();
        let site = dir.path().join("site");
        std::fs::create_dir_all(site.join("css")).unwrap();
        std::fs::write(
            site.join("index.html"),
            "<html><head></head><body>hi</body></html>",
        )
        .unwrap();
        std::fs::write(site.join("css/a b.css"), "body{}").unwrap();
        std::fs::write(dir.path().join("secret.txt"), "secret").unwrap();
        dir
    }

    #[test]
    fn a_page_s_scope_serves_what_lies_under_its_directory() {
        let dir = tree();
        let scope = Scope::page(dir.path().join("site/index.html"));
        let found = scope.resolve("css/a%20b.css").unwrap();
        assert!(found.ends_with("site/css/a b.css"));
    }

    #[test]
    fn a_page_s_scope_refuses_to_climb_out_of_its_directory() {
        let dir = tree();
        let scope = Scope::page(dir.path().join("site/index.html"));
        assert_eq!(scope.resolve("../secret.txt"), None);
        assert_eq!(scope.resolve("%2e%2e/secret.txt"), None);
        assert_eq!(scope.resolve("css/%2E%2E/%2E%2E/secret.txt"), None);
        assert_eq!(scope.resolve("..%2fsecret.txt"), None);
    }

    #[cfg(unix)]
    #[test]
    fn a_page_s_scope_refuses_a_link_that_leads_out_of_its_directory() {
        let dir = tree();
        std::os::unix::fs::symlink(dir.path().join("secret.txt"), dir.path().join("site/s"))
            .unwrap();
        let scope = Scope::page(dir.path().join("site/index.html"));
        assert_eq!(scope.resolve("s"), None);
    }

    #[test]
    fn a_page_s_scope_refuses_absolute_paths_and_what_is_missing() {
        let dir = tree();
        let scope = Scope::page(dir.path().join("site/index.html"));
        let abs = encode(&dir.path().join("secret.txt").to_string_lossy());
        assert_eq!(scope.resolve(&abs), None);
        assert_eq!(scope.resolve("nothing.css"), None);
        assert_eq!(scope.resolve("css"), None);
    }

    #[test]
    fn a_file_s_scope_serves_that_file_and_no_other() {
        let dir = tree();
        let scope = Scope::File(dir.path().join("site/index.html"));
        assert!(scope.resolve("index.html").is_some());
        assert!(scope.resolve("index.html?t=1").is_some());
        assert_eq!(scope.resolve("css/a%20b.css"), None);
        assert_eq!(scope.resolve("../secret.txt"), None);
    }

    #[test]
    fn names_survive_being_put_in_a_url() {
        for name in ["a b.png", "画像 #1?.png", "100%.mov"] {
            let url = encode(name);
            assert!(!url.contains([' ', '#', '?']), "{url}");
            assert_eq!(decode(&url).as_deref(), Some(name));
        }
        assert_eq!(decode("%zz"), None);
        assert_eq!(decode("a%00b"), None);
    }

    #[test]
    fn a_range_asks_for_bytes_from_a_start_to_an_end() {
        assert_eq!(byte_range("bytes=0-1", 100), Some((0, 1)));
        assert_eq!(byte_range("bytes=10-", 100), Some((10, 99)));
        assert_eq!(byte_range("bytes=-10", 100), Some((90, 99)));
        assert_eq!(byte_range("bytes=90-500", 100), Some((90, 99)));
        assert_eq!(byte_range("bytes=0-1, 5-6", 100), Some((0, 1)));
    }

    #[test]
    fn a_range_outside_the_file_cannot_be_served() {
        assert_eq!(byte_range("bytes=100-", 100), None);
        assert_eq!(byte_range("bytes=5-1", 100), None);
        assert_eq!(byte_range("bytes=-0", 100), None);
        assert_eq!(byte_range("bytes=0-", 0), None);
        assert_eq!(byte_range("lines=0-1", 100), None);
    }

    #[test]
    fn a_long_range_is_served_a_piece_at_a_time() {
        let len = 100 * MOST_BYTES;
        assert_eq!(byte_range("bytes=0-", len), Some((0, MOST_BYTES - 1)));
    }

    #[test]
    fn the_page_gets_its_base_and_the_script_first_in_its_head() {
        let page = page_document(
            "<!doctype html><HTML><HEAD lang=\"en\"><link href=\"a.css\"></HEAD><body></body>",
            "dioxus://index.html/v-1/",
        );
        assert!(
            page.starts_with(
                "<!doctype html><HTML><HEAD lang=\"en\"><base href=\"dioxus://index.html/v-1/\"><script>"
            ),
            "{page}"
        );
        assert!(page.ends_with("<link href=\"a.css\"></HEAD><body></body>"));
    }

    #[test]
    fn a_page_without_a_head_gets_them_after_its_doctype_or_at_its_start() {
        let page = page_document("<!DOCTYPE html><p>hi</p>", "b/");
        assert!(
            page.starts_with("<!DOCTYPE html><base href=\"b/\"><script>"),
            "{page}"
        );
        let bare = page_document("<p>hi</p>", "b/");
        assert!(bare.starts_with("<base href=\"b/\"><script>") && bare.ends_with("<p>hi</p>"));
    }

    #[test]
    fn the_page_and_what_it_links_to_are_served_as_they_are() {
        let dir = tree();
        let scope = Scope::page(dir.path().join("site/index.html"));
        let page = respond(&scope, &scope.entry_url(), None);
        assert_eq!(page.status(), StatusCode::OK);
        assert_eq!(
            page.headers()[header::CONTENT_TYPE],
            "text/html; charset=utf-8"
        );
        assert!(!String::from_utf8_lossy(page.body()).contains("adroitViewer"));
        let css = respond(&scope, "css/a%20b.css", None);
        assert_eq!(css.body(), b"body{}");
    }

    #[test]
    fn a_ranged_request_is_answered_with_part_of_the_file() {
        let dir = tree();
        let scope = Scope::File(dir.path().join("secret.txt"));
        let r = respond(&scope, "secret.txt", Some("bytes=1-3"));
        assert_eq!(r.status(), StatusCode::PARTIAL_CONTENT);
        assert_eq!(r.body(), b"ecr");
        assert_eq!(r.headers()[header::CONTENT_RANGE], "bytes 1-3/6");
        let r = respond(&scope, "secret.txt", Some("bytes=9-"));
        assert_eq!(r.status(), StatusCode::RANGE_NOT_SATISFIABLE);
    }

    #[test]
    fn what_is_outside_the_scope_is_not_found() {
        let dir = tree();
        let scope = Scope::page(dir.path().join("site/index.html"));
        assert_eq!(
            respond(&scope, "../secret.txt", None).status(),
            StatusCode::NOT_FOUND
        );
    }
}
