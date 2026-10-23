//! What opens a file: Neovim, or the viewer of its kind.

use std::path::Path;

/// A file's kind, which decides what opens it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum FileKind {
    /// Read and written in Neovim; every kind not named below.
    Text,
    Image,
    Movie,
    Html,
    Pdf,
}

impl FileKind {
    /// The kind of the file at `path`, by its extension.
    pub fn of(path: impl AsRef<Path>) -> FileKind {
        let Some(ext) = extension(path.as_ref()) else {
            return FileKind::Text;
        };
        match ext.as_str() {
            "png" | "jpg" | "jpeg" | "gif" | "webp" | "bmp" | "svg" | "ico" | "tif" | "tiff"
            | "avif" | "heic" => FileKind::Image,
            "mp4" | "m4v" | "mov" | "webm" => FileKind::Movie,
            "html" | "htm" => FileKind::Html,
            "pdf" => FileKind::Pdf,
            _ => FileKind::Text,
        }
    }

    /// Whether a viewer of the window shows the file, rather than Neovim.
    pub fn has_viewer(self) -> bool {
        self != FileKind::Text
    }
}

fn extension(path: &Path) -> Option<String> {
    Some(path.extension()?.to_str()?.to_ascii_lowercase())
}

/// The media type the webview is told for the file at `path`.
pub fn media_type(path: &Path) -> &'static str {
    match extension(path).as_deref() {
        Some("html" | "htm") => "text/html; charset=utf-8",
        Some("css") => "text/css; charset=utf-8",
        Some("js" | "mjs") => "text/javascript; charset=utf-8",
        Some("json" | "map") => "application/json",
        Some("txt" | "md") => "text/plain; charset=utf-8",
        Some("xml") => "application/xml",
        Some("svg") => "image/svg+xml",
        Some("png") => "image/png",
        Some("jpg" | "jpeg") => "image/jpeg",
        Some("gif") => "image/gif",
        Some("webp") => "image/webp",
        Some("bmp") => "image/bmp",
        Some("ico") => "image/x-icon",
        Some("tif" | "tiff") => "image/tiff",
        Some("avif") => "image/avif",
        Some("heic") => "image/heic",
        Some("mp4" | "m4v") => "video/mp4",
        Some("mov") => "video/quicktime",
        Some("webm") => "video/webm",
        Some("mp3") => "audio/mpeg",
        Some("wav") => "audio/wav",
        Some("ogg") => "audio/ogg",
        Some("woff") => "font/woff",
        Some("woff2") => "font/woff2",
        Some("ttf") => "font/ttf",
        Some("otf") => "font/otf",
        Some("wasm") => "application/wasm",
        Some("pdf") => "application/pdf",
        _ => "application/octet-stream",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pictures_open_in_the_image_viewer() {
        for name in [
            "a.png", "a.jpg", "a.jpeg", "a.gif", "a.webp", "a.svg", "a.heic",
        ] {
            assert_eq!(FileKind::of(name), FileKind::Image, "{name}");
        }
    }

    #[test]
    fn movies_open_in_the_movie_viewer() {
        for name in ["a.mp4", "a.m4v", "a.mov", "a.webm"] {
            assert_eq!(FileKind::of(name), FileKind::Movie, "{name}");
        }
    }

    #[test]
    fn html_pages_open_in_the_html_viewer() {
        assert_eq!(FileKind::of("report/index.html"), FileKind::Html);
        assert_eq!(FileKind::of("old.htm"), FileKind::Html);
    }

    #[test]
    fn pdfs_open_in_the_pdf_viewer() {
        assert_eq!(FileKind::of("paper.pdf"), FileKind::Pdf);
    }

    #[test]
    fn the_extension_is_matched_whatever_its_case() {
        assert_eq!(FileKind::of("SHOT.PNG"), FileKind::Image);
        assert_eq!(FileKind::of("Clip.MoV"), FileKind::Movie);
        assert_eq!(FileKind::of("Paper.Pdf"), FileKind::Pdf);
    }

    #[test]
    fn source_and_unknown_files_open_in_neovim() {
        for name in [
            "main.rs",
            "README.md",
            "data.bin",
            "Makefile",
            ".envrc",
            "a.png.txt",
        ] {
            assert_eq!(FileKind::of(name), FileKind::Text, "{name}");
            assert!(!FileKind::of(name).has_viewer());
        }
    }

    #[test]
    fn a_directory_named_like_a_picture_does_not_make_its_files_pictures() {
        assert_eq!(FileKind::of("shots.png/notes"), FileKind::Text);
    }

    #[test]
    fn files_are_served_with_the_media_type_their_extension_names() {
        assert_eq!(media_type(Path::new("a.HTML")), "text/html; charset=utf-8");
        assert_eq!(media_type(Path::new("clip.mov")), "video/quicktime");
        assert_eq!(
            media_type(Path::new("style.css")),
            "text/css; charset=utf-8"
        );
        assert_eq!(media_type(Path::new("blob")), "application/octet-stream");
    }
}
