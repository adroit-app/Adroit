//! A PDF's pages as pictures, drawn by the system.
//!
//! The PDF viewer shows one page at a time and turns pages by keys, so it
//! draws each page itself rather than handing the document to the webview,
//! whose own PDF view takes the keys and gives no page to turn to.

use std::path::Path;

/// How many pages the PDF at `path` has; None when it cannot be read.
pub fn page_count(path: &Path) -> Option<usize> {
    imp::page_count(path)
}

/// Page `index` (from 0) of the PDF at `path`, drawn `width` pixels wide,
/// as a PNG.
pub fn page_png(path: &Path, index: usize, width: u32) -> Option<Vec<u8>> {
    imp::page_png(path, index, width)
}

#[cfg(target_os = "macos")]
mod imp {
    use std::path::Path;

    use objc2::AnyThread;
    use objc2::rc::Retained;
    use objc2_app_kit::{NSBitmapImageFileType, NSBitmapImageRep};
    use objc2_foundation::{NSDictionary, NSSize, NSString, NSURL};
    use objc2_pdf_kit::{PDFDisplayBox, PDFDocument};

    fn document(path: &Path) -> Option<Retained<PDFDocument>> {
        let url = NSURL::fileURLWithPath(&NSString::from_str(path.to_str()?));
        unsafe { PDFDocument::initWithURL(PDFDocument::alloc(), &url) }
    }

    pub fn page_count(path: &Path) -> Option<usize> {
        Some(unsafe { document(path)?.pageCount() })
    }

    pub fn page_png(path: &Path, index: usize, width: u32) -> Option<Vec<u8>> {
        let doc = document(path)?;
        let page = unsafe { doc.pageAtIndex(index) }?;
        let bounds = unsafe { page.boundsForBox(PDFDisplayBox::MediaBox) };
        if bounds.size.width <= 0.0 || bounds.size.height <= 0.0 {
            return None;
        }
        let width = f64::from(width.max(1));
        let size = NSSize::new(width, width * bounds.size.height / bounds.size.width);
        let image = unsafe { page.thumbnailOfSize_forBox(size, PDFDisplayBox::MediaBox) };
        let tiff = image.TIFFRepresentation()?;
        let rep = NSBitmapImageRep::imageRepWithData(&tiff)?;
        let png = unsafe {
            rep.representationUsingType_properties(NSBitmapImageFileType::PNG, &NSDictionary::new())
        }?;
        Some(png.to_vec())
    }
}

#[cfg(not(target_os = "macos"))]
mod imp {
    use std::path::Path;

    pub fn page_count(_path: &Path) -> Option<usize> {
        None
    }

    pub fn page_png(_path: &Path, _index: usize, _width: u32) -> Option<Vec<u8>> {
        None
    }
}

#[cfg(all(test, target_os = "macos"))]
mod tests {
    use super::*;

    /// A PDF of two blank pages, the second twice as tall as the first.
    const TWO_PAGES: &str = "%PDF-1.4
1 0 obj << /Type /Catalog /Pages 2 0 R >> endobj
2 0 obj << /Type /Pages /Kids [3 0 R 4 0 R] /Count 2 >> endobj
3 0 obj << /Type /Page /Parent 2 0 R /MediaBox [0 0 200 100] >> endobj
4 0 obj << /Type /Page /Parent 2 0 R /MediaBox [0 0 200 400] >> endobj
trailer << /Root 1 0 R >>
%%EOF
";

    fn two_pages() -> tempfile::TempDir {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("a.pdf"), TWO_PAGES).unwrap();
        dir
    }

    #[test]
    fn a_pdf_s_pages_are_counted() {
        let dir = two_pages();
        assert_eq!(page_count(&dir.path().join("a.pdf")), Some(2));
    }

    #[test]
    fn a_file_that_is_no_pdf_has_no_pages() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("a.pdf"), "not a pdf").unwrap();
        assert_eq!(page_count(&dir.path().join("a.pdf")), None);
        assert_eq!(page_count(&dir.path().join("missing.pdf")), None);
    }

    #[test]
    fn a_page_is_drawn_as_a_png_in_its_own_proportions() {
        let dir = two_pages();
        let png = page_png(&dir.path().join("a.pdf"), 1, 300).unwrap();
        let image = image::load_from_memory(&png).unwrap();
        let ratio = f64::from(image.height()) / f64::from(image.width());
        assert!(
            (ratio - 2.0).abs() < 0.05,
            "{}x{}",
            image.width(),
            image.height()
        );
    }

    #[test]
    fn a_page_past_the_last_is_not_drawn() {
        let dir = two_pages();
        assert_eq!(page_png(&dir.path().join("a.pdf"), 2, 300), None);
    }
}
