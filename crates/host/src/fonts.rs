//! The monospace font families installed on this machine, for the console to
//! be drawn in.

use adroit_core::fonts::{equal_widths, monospace_families};

/// The monospace families installed on this machine, as the system names
/// them. Reads every font the system has, so it is slow enough to keep off
/// the window's thread.
pub fn installed() -> Vec<String> {
    let mut db = fontdb::Database::new();
    db.load_system_fonts();
    monospace_families(db.faces().filter_map(|f| {
        let mono = f.monospaced
            || db
                .with_face_data(f.id, |data, index| {
                    ttf_parser::Face::parse(data, index).is_ok_and(|face| draws_equal_cells(&face))
                })
                .unwrap_or(false);
        Some((f.families.first()?.0.as_str(), mono))
    }))
}

/// Whether `face` draws every letter and digit as wide, which is what the
/// console's grid needs. The font's own fixed-pitch flag is not enough:
/// Monaco, for one, leaves it unset.
fn draws_equal_cells(face: &ttf_parser::Face) -> bool {
    equal_widths(
        ('0'..='9')
            .chain('A'..='Z')
            .chain('a'..='z')
            .map(|c| face.glyph_index(c).and_then(|g| face.glyph_hor_advance(g))),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn adroit_s_own_font_draws_equal_cells() {
        let font = include_bytes!("../../../assets/fonts/JetBrainsMonoNerdFontMono-Regular.woff2");
        let ttf = wuff::decompress_woff2(font).unwrap();
        let face = ttf_parser::Face::parse(&ttf, 0).unwrap();
        assert!(draws_equal_cells(&face));
    }
}
