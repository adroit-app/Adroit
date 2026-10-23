//! The file tree's icons: the Nerd Font glyph each path gets, found as
//! vim-nerdfont's `nerdfont#find` finds it, and the colour vim-glyph-palette
//! gives that glyph.
//!
//! Both tables are copied into `assets/file-icons/` at a pinned revision and
//! turned into Rust by `build.rs`.

use std::path::Path;
use std::sync::LazyLock;

use regex::RegexSet;

use crate::palette as themes;

include!(concat!(env!("OUT_DIR"), "/file_icons.rs"));

/// `g:nerdfont#default`, which vim-nerdfont sets in its Vim script rather
/// than in a table.
const DEFAULT: &str = "\u{f15b}";

static PATTERNS: LazyLock<RegexSet> =
    LazyLock::new(|| RegexSet::new(PATTERN.iter().map(|(p, _)| p)).expect("PATTERN"));

/// What a path is to the file tree.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    File,
    Directory { open: bool },
}

/// The colour a glyph is drawn in.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Colour {
    /// The theme's ANSI colour of this number, from `GlyphPaletteN`.
    Ansi(u8),
    /// The colour of directories, from `GlyphPaletteDirectory`.
    Directory,
}

impl Colour {
    /// The stylesheet's value for this colour in the current theme.
    pub fn css(self) -> String {
        match self {
            Colour::Ansi(n) => format!("var({})", themes::ANSI[usize::from(n)]),
            Colour::Directory => "var(--run)".to_string(),
        }
    }
}

/// The icon the file tree draws before a path.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FileIcon {
    pub glyph: &'static str,
    /// `None` draws the glyph in the text's own colour.
    pub colour: Option<Colour>,
}

/// The icon of the path `path`, which is a `kind`.
pub fn find(path: impl AsRef<Path>, kind: Kind) -> FileIcon {
    let glyph = glyph(path.as_ref(), kind);
    FileIcon {
        glyph,
        colour: colour(glyph),
    }
}

/// `nerdfont#find`: the first pattern the path matches, then its name, then
/// a folder for a directory, then its extension.
fn glyph(path: &Path, kind: Kind) -> &'static str {
    let path = path.to_string_lossy();
    match kind {
        Kind::File => {
            let name = path.rsplit('/').next().unwrap_or_default().to_lowercase();
            pattern(&path)
                .or_else(|| lookup(BASENAME, &name))
                .or_else(|| extension(&name).and_then(|e| lookup(EXTENSION, e)))
                .unwrap_or(DEFAULT)
        }
        // vim-nerdfont's directory patterns end in `.$`, written for a path
        // with a trailing slash, which leaves a directory no name to match.
        Kind::Directory { open } => {
            let path = format!("{}/", path.trim_end_matches('/'));
            pattern(&path)
                .or_else(|| lookup(DIRECTORY, if open { "open" } else { "close" }))
                .unwrap_or(DEFAULT)
        }
    }
}

fn pattern(path: &str) -> Option<&'static str> {
    let i = PATTERNS.matches(path).into_iter().next()?;
    Some(PATTERN[i].1)
}

/// Vim's `:e`: what follows the last dot of a name, where a leading dot
/// starts no extension.
fn extension(name: &str) -> Option<&str> {
    let name = name.strip_prefix('.').unwrap_or(name);
    name.rsplit_once('.').map(|(_, e)| e)
}

fn lookup(table: &[(&'static str, &'static str)], key: &str) -> Option<&'static str> {
    table
        .binary_search_by(|(k, _)| (*k).cmp(key))
        .ok()
        .map(|i| table[i].1)
}

fn colour(glyph: &str) -> Option<Colour> {
    PALETTE
        .binary_search_by(|(g, _)| (*g).cmp(glyph))
        .ok()
        .map(|i| PALETTE[i].1)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn glyph_of(path: &str) -> &'static str {
        find(path, Kind::File).glyph
    }

    #[test]
    fn a_pattern_beats_the_basename() {
        // `vagrantfile` is also a basename, of another glyph.
        assert_eq!(glyph_of("/repo/Vagrantfile"), "\u{f2b8}");
    }

    #[test]
    fn a_pattern_beats_the_extension() {
        assert_eq!(glyph_of("/repo/src/index.d.ts"), "\u{e69d}");
        assert_eq!(glyph_of("/repo/src/index.ts"), "\u{e628}");
    }

    #[test]
    fn the_basename_beats_the_extension() {
        assert_eq!(glyph_of("/repo/README.md"), "\u{f00ba}");
        assert_eq!(glyph_of("/repo/NOTES.md"), "\u{f48a}");
    }

    #[test]
    fn the_basename_and_the_extension_are_matched_in_lower_case() {
        assert_eq!(glyph_of("/repo/Makefile"), "\u{e673}");
        assert_eq!(glyph_of("/repo/LOGO.PNG"), "\u{f1c5}");
    }

    #[test]
    fn the_extension_is_the_last_one() {
        assert_eq!(glyph_of("/repo/dist/app.tar.gz"), "\u{f410}");
    }

    #[test]
    fn a_leading_dot_does_not_start_an_extension() {
        assert_eq!(glyph_of("/home/me/.json"), DEFAULT);
    }

    #[test]
    fn a_pattern_matches_the_whole_path_case_sensitively() {
        assert_eq!(glyph_of("/repo/.env.local"), "\u{f462}");
        assert_eq!(glyph_of("/repo/.ENV"), DEFAULT);
    }

    #[test]
    fn a_file_no_table_knows_gets_the_default_glyph() {
        assert_eq!(glyph_of("/repo/unknown.zzz"), DEFAULT);
        assert_eq!(glyph_of("/repo/no-extension"), DEFAULT);
    }

    #[test]
    fn a_directory_is_an_open_or_closed_folder() {
        let open = find("/repo/src", Kind::Directory { open: true });
        let closed = find("/repo/src", Kind::Directory { open: false });
        assert_eq!(open.glyph, "\u{e5fe}");
        assert_eq!(closed.glyph, "\u{e5ff}");
    }

    #[test]
    fn a_directory_is_matched_by_the_patterns_written_for_directories() {
        let bin = find("/usr/bin", Kind::Directory { open: false });
        assert_eq!(bin.glyph, "\u{e5fc}");
        let git = find("/repo/.git", Kind::Directory { open: true });
        assert_eq!(git.glyph, "\u{e5fb}");
    }

    #[test]
    fn a_directory_is_not_given_an_extension_s_glyph() {
        let dir = find("/repo/notes.md", Kind::Directory { open: false });
        assert_eq!(dir.glyph, "\u{e5ff}");
    }

    #[test]
    fn a_glyph_in_glyph_palette_n_takes_ansi_colour_n() {
        // .gitignore's glyph is in GlyphPalette1, JSON's in GlyphPalette3.
        assert_eq!(
            find("/repo/.gitignore", Kind::File).colour,
            Some(Colour::Ansi(1))
        );
        assert_eq!(
            find("/repo/a.json", Kind::File).colour,
            Some(Colour::Ansi(3))
        );
        assert_eq!(find("/repo/a.ts", Kind::File).colour, Some(Colour::Ansi(4)));
    }

    #[test]
    fn a_folder_takes_the_directory_colour() {
        let dir = find("/repo/src", Kind::Directory { open: false });
        assert_eq!(dir.colour, Some(Colour::Directory));
    }

    #[test]
    fn a_glyph_in_no_palette_takes_no_colour() {
        // Rust's glyph is in none of glyph-palette's lists.
        assert_eq!(find("/repo/main.rs", Kind::File).colour, None);
    }

    #[test]
    fn the_default_glyph_has_the_colour_glyph_palette_gives_it() {
        assert_eq!(
            find("/repo/x.zzz", Kind::File).colour,
            Some(Colour::Ansi(7))
        );
    }

    #[test]
    fn a_palette_colour_is_the_theme_s_ansi_colour_and_directories_are_running_blue() {
        assert_eq!(Colour::Ansi(1).css(), "var(--ansi-1)");
        assert_eq!(Colour::Ansi(9).css(), "var(--ansi-9)");
        assert_eq!(Colour::Directory.css(), "var(--run)");
    }

    #[test]
    fn every_palette_number_has_a_theme_variable() {
        for (_, colour) in PALETTE {
            if let Colour::Ansi(n) = colour {
                assert!(usize::from(*n) < themes::ANSI.len(), "{n}");
            }
        }
    }

    #[test]
    fn every_pattern_reads_as_a_regular_expression() {
        assert_eq!(PATTERNS.len(), PATTERN.len());
    }

    /// Every glyph the tables can give a path.
    fn every_glyph() -> impl Iterator<Item = &'static str> {
        [BASENAME, EXTENSION, DIRECTORY, PATTERN]
            .into_iter()
            .flatten()
            .map(|(_, g)| *g)
            .chain([DEFAULT])
    }

    #[test]
    fn every_private_use_glyph_is_in_the_bundled_font() {
        let font = include_bytes!("../../../assets/fonts/JetBrainsMonoNerdFontMono-Regular.woff2");
        let ttf = wuff::decompress_woff2(font).unwrap();
        let face = ttf_parser::Face::parse(&ttf, 0).unwrap();
        let missing: Vec<String> = every_glyph()
            .flat_map(str::chars)
            .filter(|c| matches!(u32::from(*c), 0xe000..=0xf8ff | 0xf0000..=0x10ffff))
            .filter(|c| face.glyph_index(*c).is_none())
            .map(|c| format!("U+{:04X}", u32::from(c)))
            .collect();
        assert!(missing.is_empty(), "{missing:?}");
    }
}
