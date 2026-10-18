//! The console's colour themes: those adroit comes with, and the user's own
//! in the themes folder, both in Ghostty's format.

use std::path::Path;

use adroit_term::{Palette, Rgb};

/// The theme of adroit's own palette.
pub const DEFAULT: &str = "Adroit";

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
    let file = dir.join(name);
    if !name.contains('/') && file.is_file() {
        let text =
            std::fs::read_to_string(&file).map_err(|e| format!("{}: {e}", file.display()))?;
        return Palette::from_ghostty(&text).map_err(|e| format!("{}: {e}", file.display()));
    }
    if name == DEFAULT {
        return Ok(Palette::default());
    }
    PRESETS
        .iter()
        .find(|(n, _)| *n == name)
        .map(|(_, text)| Palette::from_ghostty(text))
        .unwrap_or_else(|| Err(format!("no theme named {name:?}")))
}

/// The window's colours in a theme: the stylesheet's variables, by name,
/// and whether the theme is a dark one.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Window {
    pub vars: Vec<(&'static str, String)>,
    pub dark: bool,
}

/// The window's colours drawn from a palette: its background for the panels,
/// raised parts and lines a little toward its foreground, the ground a little
/// away from it, quieter text toward the background, and the states in its
/// own yellow, blue, green, red and magenta.
pub fn window(p: &Palette) -> Window {
    let (bg, fg) = (p.background, p.foreground);
    let toward = |amount: f64| mix(bg, fg, amount).hex();
    let away = mix(bg, Rgb::new(0, 0, 0), 0.25);
    let lum = |c: Rgb| 0.2126 * c.r as f64 + 0.7152 * c.g as f64 + 0.0722 * c.b as f64;
    let dark = lum(bg) < lum(fg);
    // On a light background the ground darkens less, or it reads as a border.
    let ground = if dark { away } else { mix(bg, fg, -0.04) };
    Window {
        vars: vec![
            ("--ground", ground.hex()),
            ("--panel", bg.hex()),
            ("--raised", toward(0.06)),
            ("--select", toward(0.12)),
            ("--line", toward(0.09)),
            ("--line-strong", toward(0.17)),
            ("--text", fg.hex()),
            ("--text-2", toward(0.72)),
            ("--text-3", toward(0.55)),
            ("--wait", p.ansi[3].hex()),
            ("--run", p.ansi[4].hex()),
            ("--done", p.ansi[2].hex()),
            ("--fail", p.ansi[1].hex()),
            ("--coord", p.ansi[5].hex()),
        ],
        dark,
    }
}

/// `from` moved `amount` of the way to `to`; a negative amount moves away.
fn mix(from: Rgb, to: Rgb, amount: f64) -> Rgb {
    let at = |a: u8, b: u8| {
        (a as f64 + (b as f64 - a as f64) * amount)
            .round()
            .clamp(0.0, 255.0) as u8
    };
    Rgb::new(at(from.r, to.r), at(from.g, to.g), at(from.b, to.b))
}

/// `window`'s colours as declarations for the stylesheet's root.
pub fn css(window: &Window) -> String {
    let mut out: String = window
        .vars
        .iter()
        .map(|(name, value)| format!("{name}:{value};"))
        .collect();
    let scheme = if window.dark { "dark" } else { "light" };
    out.push_str(&format!("color-scheme:{scheme};"));
    out
}

#[cfg(test)]
mod tests {
    use adroit_term::Rgb;

    use super::*;

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

    fn var<'a>(window: &'a Window, name: &str) -> &'a str {
        window
            .vars
            .iter()
            .find(|(n, _)| *n == name)
            .map(|(_, v)| v.as_str())
            .unwrap()
    }

    #[test]
    fn the_window_is_drawn_on_the_theme_s_background_in_its_foreground() {
        let p = palette("Nightfox", Path::new("/no/such/folder")).unwrap();
        let w = window(&p);
        assert_eq!(var(&w, "--panel"), "#192330");
        assert_eq!(var(&w, "--text"), "#cdcecf");
        assert_eq!(var(&w, "--wait"), p.ansi[3].hex());
        assert_eq!(var(&w, "--fail"), p.ansi[1].hex());
        assert!(w.dark);
    }

    #[test]
    fn raised_parts_lean_toward_the_text_and_the_ground_away_from_it() {
        let lum = |hex: &str| {
            Rgb::parse(hex)
                .map(|c| c.r as u32 + c.g as u32 + c.b as u32)
                .unwrap()
        };
        for name in ["Nightfox", "Catppuccin Latte"] {
            let p = palette(name, Path::new("/no/such/folder")).unwrap();
            let w = window(&p);
            let toward_text = lum(var(&w, "--text")) > lum(var(&w, "--panel"));
            let steps = [
                "--ground", "--panel", "--raised", "--select", "--text-3", "--text-2", "--text",
            ];
            for pair in steps.windows(2) {
                let (a, b) = (lum(var(&w, pair[0])), lum(var(&w, pair[1])));
                assert!(
                    if toward_text { a <= b } else { a >= b },
                    "{name}: {pair:?}"
                );
            }
            assert_eq!(w.dark, toward_text, "{name}");
        }
    }

    #[test]
    fn the_window_s_colours_are_declarations_for_the_stylesheet() {
        let w = Window {
            vars: vec![("--panel", "#192330".into()), ("--text", "#cdcecf".into())],
            dark: true,
        };
        assert_eq!(css(&w), "--panel:#192330;--text:#cdcecf;color-scheme:dark;");
        let light = Window { dark: false, ..w };
        assert!(css(&light).ends_with("color-scheme:light;"));
    }

    #[test]
    fn a_theme_no_one_has_is_a_mistake() {
        assert!(palette("Nowhere", Path::new("/no/such/folder")).is_err());
    }
}
