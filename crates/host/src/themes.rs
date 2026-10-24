//! The console's colour themes: those adroit comes with, and the user's own
//! in the themes folder, both in Ghostty's format.

use std::path::Path;

use adroit_protocol::{Colours, ThemeKind, ThemeListed};
use adroit_term::Palette;

use crate::settings::Settings;

pub use adroit_core::palette::ANSI;

/// The themes adroit comes with, by name: the dark ones, then the light.
const PRESETS: &[(&str, &str)] = &[
    ("Nightfox", include_str!("../themes/Nightfox")),
    ("Duskfox", include_str!("../themes/Duskfox")),
    ("Nordfox", include_str!("../themes/Nordfox")),
    ("Terafox", include_str!("../themes/Terafox")),
    ("Carbonfox", include_str!("../themes/Carbonfox")),
    ("TokyoNight", include_str!("../themes/TokyoNight")),
    (
        "Catppuccin Mocha",
        include_str!("../themes/Catppuccin Mocha"),
    ),
    ("Gruvbox Dark", include_str!("../themes/Gruvbox Dark")),
    (
        "Gruvbox Material Dark",
        include_str!("../themes/Gruvbox Material Dark"),
    ),
    ("Dracula", include_str!("../themes/Dracula")),
    ("Nord", include_str!("../themes/Nord")),
    ("Kanagawa Wave", include_str!("../themes/Kanagawa Wave")),
    ("Rose Pine", include_str!("../themes/Rose Pine")),
    ("Dayfox", include_str!("../themes/Dayfox")),
    ("Dawnfox", include_str!("../themes/Dawnfox")),
    ("TokyoNight Day", include_str!("../themes/TokyoNight Day")),
    (
        "Catppuccin Latte",
        include_str!("../themes/Catppuccin Latte"),
    ),
    ("Gruvbox Light", include_str!("../themes/Gruvbox Light")),
    (
        "Gruvbox Material Light",
        include_str!("../themes/Gruvbox Material Light"),
    ),
    (
        "Everforest Light Med",
        include_str!("../themes/Everforest Light Med"),
    ),
    ("Kanagawa Lotus", include_str!("../themes/Kanagawa Lotus")),
    ("Rose Pine Dawn", include_str!("../themes/Rose Pine Dawn")),
    (
        "GitHub Light Default",
        include_str!("../themes/GitHub Light Default"),
    ),
    ("One Half Light", include_str!("../themes/One Half Light")),
    ("Melange Light", include_str!("../themes/Melange Light")),
    ("Zenbones Light", include_str!("../themes/Zenbones Light")),
];

/// Every theme to choose from: those adroit comes with, then the user's in
/// `dir` by name.
pub fn names(dir: &Path) -> Vec<String> {
    let mut out: Vec<String> = PRESETS.iter().map(|(n, _)| n.to_string()).collect();
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

/// Whether the theme `name` is light or dark, by its palette; one that
/// cannot be read counts as dark.
pub fn kind(name: &str, dir: &Path) -> ThemeKind {
    match palette(name, dir) {
        Ok(p) if !p.is_dark() => ThemeKind::Light,
        _ => ThemeKind::Dark,
    }
}

/// Every theme to choose from, as [`names`] lists them, with its kind.
pub fn listed(dir: &Path) -> Vec<ThemeListed> {
    names(dir)
        .into_iter()
        .map(|name| ThemeListed {
            kind: kind(&name, dir),
            name,
        })
        .collect()
}

/// `settings` with the one theme an earlier version named taken by its
/// kind, read from `dir`.
pub fn migrated(settings: Settings, dir: &Path) -> Settings {
    match settings.theme.as_deref() {
        Some(theme) => {
            let kind = kind(theme, dir);
            settings.migrated(kind)
        }
        None => settings,
    }
}

/// The theme `settings` name for the system's appearance now: the one
/// rule every session, shell and window is coloured by.
pub fn in_force(settings: &Settings) -> String {
    settings.theme_in(crate::appearance::system()).to_string()
}

/// The palette of the theme in force, from the user's themes folder or
/// adroit's.
pub fn palette_in_force(settings: &Settings) -> Result<Palette, String> {
    palette(&in_force(settings), &crate::settings::themes_dir())
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
    fn a_theme_s_colours_are_its_text_or_why_it_cannot_be_had() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("Mine"), "background = #010203\n").unwrap();
        assert_eq!(
            colours("Adroit", dir.path()),
            Colours::Unreadable {
                error: "no theme named \"Adroit\"".into()
            }
        );
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
    fn the_themes_adroit_comes_with_are_as_many_light_as_dark_the_darks_first() {
        let none = Path::new("/no/such/folder");
        let of = |k: ThemeKind| -> Vec<String> {
            listed(none)
                .into_iter()
                .filter(|t| t.kind == k)
                .map(|t| t.name)
                .collect()
        };
        assert_eq!(
            of(ThemeKind::Dark),
            [
                "Nightfox",
                "Duskfox",
                "Nordfox",
                "Terafox",
                "Carbonfox",
                "TokyoNight",
                "Catppuccin Mocha",
                "Gruvbox Dark",
                "Gruvbox Material Dark",
                "Dracula",
                "Nord",
                "Kanagawa Wave",
                "Rose Pine",
            ]
        );
        assert_eq!(
            of(ThemeKind::Light),
            [
                "Dayfox",
                "Dawnfox",
                "TokyoNight Day",
                "Catppuccin Latte",
                "Gruvbox Light",
                "Gruvbox Material Light",
                "Everforest Light Med",
                "Kanagawa Lotus",
                "Rose Pine Dawn",
                "GitHub Light Default",
                "One Half Light",
                "Melange Light",
                "Zenbones Light",
            ]
        );
        let names = names(none);
        let first_light = names.iter().position(|n| n == "Dayfox").unwrap();
        assert!(
            names[first_light..]
                .iter()
                .all(|n| kind(n, none) == ThemeKind::Light)
        );
        assert_eq!(kind("Nowhere", none), ThemeKind::Dark);
    }

    #[test]
    fn every_theme_adroit_comes_with_names_its_licence() {
        let readme = include_str!("../themes/README.md");
        for (name, _) in PRESETS {
            assert!(readme.contains(name), "{name}");
        }
    }

    #[test]
    fn the_light_theme_named_when_none_is_is_one_adroit_comes_with_and_light() {
        let none = Path::new("/no/such/folder");
        let defaults = Settings::default();
        assert_eq!(kind(&defaults.light_theme, none), ThemeKind::Light);
        assert_eq!(kind(&defaults.dark_theme, none), ThemeKind::Dark);
    }

    #[test]
    fn the_one_theme_an_earlier_version_named_keeps_its_look() {
        use crate::settings::Appearance;
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(
            dir.path().join("Paper"),
            "background = #fafafa\nforeground = #202020\n",
        )
        .unwrap();
        let read = |text: &str| migrated(Settings::parse(text).unwrap(), dir.path());
        let s = read("theme = \"Paper\"\n");
        assert_eq!(
            (s.appearance, s.light_theme.as_str(), s.theme),
            (Appearance::Light, "Paper", None)
        );
        let s = read("theme = \"Nord\"\n");
        assert_eq!(
            (s.appearance, s.dark_theme.as_str()),
            (Appearance::Dark, "Nord")
        );
        let s = read("theme = \"Adroit\"\n");
        assert_eq!(
            (s.appearance, s.dark_theme.as_str()),
            (Appearance::Dark, "Nightfox")
        );
        let s = read("theme = \"Gone\"\n");
        assert_eq!(
            (s.appearance, s.dark_theme.as_str()),
            (Appearance::Dark, "Gone")
        );
        assert_eq!(read(""), Settings::default());
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
        assert_eq!(listed[0], PRESETS[0].0);
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
