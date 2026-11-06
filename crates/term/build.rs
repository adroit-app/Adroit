//! Reads which characters the console's own font draws out of its files,
//! so a screen can tell the ones a fallback font would draw instead.

use std::collections::BTreeSet;
use std::fmt::Write as _;
use std::path::PathBuf;

const STYLES: [&str; 4] = ["Regular", "Bold", "Italic", "BoldItalic"];

fn main() {
    let manifest = PathBuf::from(std::env::var_os("CARGO_MANIFEST_DIR").unwrap());
    let fonts = manifest.join("../../assets/fonts");
    println!("cargo:rerun-if-changed={}", fonts.display());
    // A character is the font's only when every style draws it: bold or
    // italic text missing it would fall back as well.
    let drawn = STYLES
        .iter()
        .map(|style| {
            let path = fonts.join(format!("JetBrainsMonoNerdFontMono-{style}.woff2"));
            let woff2 = std::fs::read(&path)
                .unwrap_or_else(|e| panic!("cannot read {}: {e}", path.display()));
            let ttf = wuff::decompress_woff2(&woff2)
                .unwrap_or_else(|e| panic!("{} is no WOFF2: {e:?}", path.display()));
            let face = ttf_parser::Face::parse(&ttf, 0)
                .unwrap_or_else(|e| panic!("{} is no font: {e}", path.display()));
            code_points(&face)
        })
        .reduce(|a, b| &a & &b)
        .unwrap();
    let mut ranges: Vec<(u32, u32)> = Vec::new();
    for c in drawn {
        match ranges.last_mut() {
            Some(last) if last.1 + 1 == c => last.1 = c,
            _ => ranges.push((c, c)),
        }
    }
    let mut out = String::from("static DRAWN: &[(u32, u32)] = &[");
    for (first, last) in ranges {
        write!(out, "(0x{first:X}, 0x{last:X}),").unwrap();
    }
    out.push_str("];\n");
    let path = PathBuf::from(std::env::var_os("OUT_DIR").unwrap()).join("drawn.rs");
    std::fs::write(path, out).unwrap();
}

/// Every code point the face's Unicode cmap maps to a glyph other than
/// `.notdef`.
fn code_points(face: &ttf_parser::Face) -> BTreeSet<u32> {
    let mut out = BTreeSet::new();
    let Some(cmap) = face.tables().cmap else {
        return out;
    };
    for table in cmap.subtables.into_iter().filter(|t| t.is_unicode()) {
        table.codepoints(|c| {
            if table.glyph_index(c).is_some_and(|g| g.0 != 0) {
                out.insert(c);
            }
        });
    }
    out
}
