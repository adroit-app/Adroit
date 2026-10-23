//! The console's colour themes: those adroit comes with, and the user's own
//! in the themes folder, both in Ghostty's format.

use std::path::Path;

use adroit_protocol::Colours;
use adroit_term::Palette;

pub use adroit_core::palette::{ANSI, DEFAULT};

/// The themes adroit comes with, by name.
const PRESETS: &[(&str, &str)] = &[
    ("Nightfox", include_str!("../themes/Nightfox")),
    ("TokyoNight", include_str!("../themes/TokyoNight")),
    (
        "Catppuccin Mocha",
        include_str!("../themes/Catppuccin Mocha"),
    ),
    ("Gruvbox Dark", include_str!("../themes/Gruvbox Dark")),
    ("Dracula", include_str!("../themes/Dracula")),
    ("Nord", include_str!("../themes/Nord")),
    ("Kanagawa Wave", include_str!("../themes/Kanagawa Wave")),
    ("Rose Pine", include_str!("../themes/Rose Pine")),
    (
        "Catppuccin Latte",
        include_str!("../themes/Catppuccin Latte"),
    ),
    (
        "GitHub Light Default",
        include_str!("../themes/GitHub Light Default"),
    ),
];

/// Every theme to choose from: adroit's own, those it comes with, then the
/// user's in `dir` by name.
pub fn names(dir: &Path) -> Vec<String> {
    let mut out: Vec<String> = std::iter::once(DEFAULT)
        .chain(PRESETS.iter().map(|(n, _)| *n))
        .map(String::from)
        .collect();
    let mut own: Vec<String> = std::fs::read_dir(dir)
        .into_iter()
        .flatten()
        .filter_map(Result::ok)
        .filter(|e| e.file_type().is_ok_and(|t| t.is_file()))
        .filter_map(|e| e.file_name().into_string().ok())
        .filter(|n| !n.starts_with('.') && !out.contains(n))
        .collect();
    own.sort();
    out.extend(own);
    out
}

/// The palette of the theme `name`: the user's file of that name in `dir`
/// first, so one can be changed by writing it there, then adroit's.
pub fn palette(name: &str, dir: &Path) -> Result<Palette, String> {
    match colours(name, dir) {
        Colours::Own => Ok(Palette::default()),
        Colours::Theme { text } => Palette::from_ghostty(&text),
        Colours::Unreadable { error } => Err(error),
    }
}

/// The colours of the theme `name`, found as [`palette`] finds them: a
/// theme's text only once it reads as a palette.
pub fn colours(name: &str, dir: &Path) -> Colours {
    let file = dir.join(name);
    let read = if !name.contains('/') && file.is_file() {
        std::fs::read_to_string(&file)
            .map_err(|e| e.to_string())
            .and_then(|text| Palette::from_ghostty(&text).map(|_| text))
            .map_err(|e| format!("{}: {e}", file.display()))
    } else if name == DEFAULT {
        return Colours::Own;
    } else {
        PRESETS
            .iter()
            .find(|(n, _)| *n == name)
            .map(|(_, text)| Palette::from_ghostty(text).map(|_| (*text).to_string()))
            .unwrap_or_else(|| Err(format!("no theme named {name:?}")))
    };
    match read {
        Ok(text) => Colours::Theme { text },
        Err(error) => Colours::Unreadable { error },
    }
}

#[cfg(test)]
mod tests {
    use adroit_term::Rgb;

    use super::*;

    #[test]
    fn a_theme_s_colours_are_adroit_s_own_its_text_or_why_it_cannot_be_had() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("Mine"), "background = #010203\n").unwrap();
        assert_eq!(colours(DEFAULT, dir.path()), Colours::Own);
        assert_eq!(
            colours("Mine", dir.path()),
            Colours::Theme {
                text: "background = #010203\n".into()
            }
        );
        assert!(matches!(colours("Nord", dir.path()), Colours::Theme { .. }));
        assert_eq!(
            colours("Nothing", dir.path()),
            Colours::Unreadable {
                error: "no theme named \"Nothing\"".into()
            }
        );
    }

    #[test]
    fn every_theme_adroit_comes_with_reads() {
        let none = Path::new("/no/such/folder");
        for name in names(none) {
            assert!(palette(&name, none).is_ok(), "{name}");
        }
    }

    #[test]
    fn nightfox_is_among_them_as_ghostty_draws_it() {
        let p = palette("Nightfox", Path::new("/no/such/folder")).unwrap();
        assert_eq!(p.background, Rgb::new(0x19, 0x23, 0x30));
        assert_eq!(p.foreground, Rgb::new(0xcd, 0xce, 0xcf));
    }

    #[test]
    fn a_theme_of_the_user_s_own_is_offered_after_adroit_s_and_found_by_its_name() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("Mine"), "background = #010203\n").unwrap();
        let listed = names(dir.path());
        assert_eq!(listed[0], DEFAULT);
        assert_eq!(listed.last().map(String::as_str), Some("Mine"));
        let p = palette("Mine", dir.path()).unwrap();
        assert_eq!(p.background, Rgb::new(1, 2, 3));
    }

    #[test]
    fn the_user_s_theme_of_a_name_adroit_has_wins_and_is_listed_once() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("Nightfox"), "background = #010203\n").unwrap();
        assert_eq!(
            names(dir.path())
                .iter()
                .filter(|n| *n == "Nightfox")
                .count(),
            1
        );
        let p = palette("Nightfox", dir.path()).unwrap();
        assert_eq!(p.background, Rgb::new(1, 2, 3));
    }

    #[test]
    fn a_theme_no_one_has_is_a_mistake() {
        assert!(palette("Nowhere", Path::new("/no/such/folder")).is_err());
    }
}
