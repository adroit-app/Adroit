//! The window's own colours, drawn from the palette of the theme the work
//! host read.

use adroit_core::palette::ANSI;
use adroit_term::{Palette, Rgb};

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
    let dark = p.is_dark();
    // On a light background the ground darkens less, or it reads as a border.
    let ground = if dark { away } else { mix(bg, fg, -0.04) };
    // Dark lines on a light ground stand out more than light lines on a
    // dark one, so a light theme draws them closer to its panels.
    let line = |amount: f64| toward(if dark { amount } else { amount * 0.65 });
    let mut vars = vec![
        ("--ground", ground.hex()),
        ("--panel", bg.hex()),
        ("--raised", toward(0.06)),
        ("--select", toward(0.12)),
        ("--line", line(0.09)),
        ("--line-strong", line(0.17)),
        ("--focus-line", line(0.20)),
        ("--focus-border", line(0.30)),
        ("--text", fg.hex()),
        ("--text-2", toward(0.72)),
        ("--text-3", toward(0.55)),
        ("--wait", p.ansi[3].hex()),
        ("--run", p.ansi[4].hex()),
        ("--done", p.ansi[2].hex()),
        ("--fail", p.ansi[1].hex()),
        ("--coord", p.ansi[5].hex()),
        ("--usage-cpu", p.ansi[6].hex()),
        ("--usage-context", p.ansi[6].hex()),
        ("--usage-gpu", p.ansi[5].hex()),
        ("--usage-memory", p.ansi[2].hex()),
        ("--usage-storage", mix(p.ansi[3], p.ansi[1], 0.25).hex()),
        ("--kbd-text", toward(0.72)),
        // A shadow as deep on a light ground as on a dark one reads as a
        // smudge rather than as depth.
        (
            "--shadow",
            format!("rgba(0,0,0,{})", if dark { 0.45 } else { 0.06 }),
        ),
    ];
    vars.extend(
        ANSI.iter()
            .zip(p.ansi)
            .map(|(name, colour)| (*name, colour.hex())),
    );
    Window { vars, dark }
}

/// A Markdown document's colours in a theme: Arto's scheme, `light` or
/// `dark`, and the declarations of Primer's variables, which Arto's
/// stylesheet takes every colour from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Document {
    pub scheme: &'static str,
    pub tokens: String,
}

/// A document's colours drawn from a palette as the window's are: it lies on
/// the window's panel in its text, its lines and raised parts are the
/// window's, and its links, alerts and code take the palette's own colours.
pub fn document(p: &Palette) -> Document {
    let w = window(p);
    let of = |name: &str| {
        w.vars
            .iter()
            .find(|(n, _)| *n == name)
            .map(|(_, v)| v.clone())
            .unwrap_or_default()
    };
    let ansi = |i: usize| p.ansi[i].hex();
    let tint = |i: usize| mix(p.background, p.ansi[i], 0.18).hex();
    let tokens: Vec<(&str, String)> = vec![
        ("--bgColor-default", of("--panel")),
        ("--bgColor-muted", of("--raised")),
        ("--bgColor-neutral-muted", of("--select")),
        ("--bgColor-inset", of("--ground")),
        ("--fgColor-default", of("--text")),
        ("--fgColor-muted", of("--text-2")),
        ("--fgColor-accent", ansi(4)),
        ("--fgColor-success", ansi(2)),
        ("--fgColor-attention", ansi(3)),
        ("--fgColor-danger", ansi(1)),
        ("--fgColor-done", ansi(5)),
        ("--fgColor-severe", mix(p.ansi[3], p.ansi[1], 0.5).hex()),
        ("--borderColor-default", of("--line-strong")),
        ("--borderColor-muted", of("--line")),
        ("--borderColor-accent-emphasis", ansi(4)),
        ("--borderColor-success-emphasis", ansi(2)),
        ("--borderColor-attention-emphasis", ansi(3)),
        ("--borderColor-danger-emphasis", ansi(1)),
        ("--borderColor-done-emphasis", ansi(5)),
        ("--bgColor-accent-muted", tint(4)),
        ("--bgColor-success-muted", tint(2)),
        ("--bgColor-attention-muted", tint(3)),
        ("--bgColor-danger-muted", tint(1)),
        ("--bgColor-done-muted", tint(5)),
        ("--prettylights-syntax-comment", of("--text-3")),
        ("--prettylights-syntax-keyword", ansi(5)),
        ("--prettylights-syntax-entity", ansi(4)),
        ("--prettylights-syntax-entityTag", ansi(2)),
        ("--prettylights-syntax-constant", ansi(6)),
        ("--prettylights-syntax-string", ansi(2)),
        ("--prettylights-syntax-stringRegexp", ansi(6)),
        ("--prettylights-syntax-variable", ansi(3)),
        ("--prettylights-syntax-storageModifierImport", of("--text")),
        ("--prettylights-syntax-markup-heading", ansi(4)),
        ("--prettylights-syntax-markup-list", ansi(3)),
        ("--prettylights-syntax-markup-bold", of("--text")),
        ("--prettylights-syntax-markup-italic", of("--text")),
        ("--prettylights-syntax-markup-inserted-text", ansi(2)),
        ("--prettylights-syntax-markup-inserted-bg", tint(2)),
        ("--prettylights-syntax-markup-deleted-text", ansi(1)),
        ("--prettylights-syntax-markup-deleted-bg", tint(1)),
        ("--prettylights-syntax-markup-changed-text", ansi(3)),
        ("--prettylights-syntax-markup-changed-bg", tint(3)),
        // A diagram lies on the raised block code lies on; its shapes are
        // filled with the panel and outlined in the quiet text, as a light
        // theme's lines are too faint to outline them.
        ("--pele-surface", of("--panel")),
        ("--pele-border", of("--text-3")),
        ("--pele-accent", ansi(4)),
    ];
    Document {
        scheme: if w.dark { "dark" } else { "light" },
        tokens: tokens
            .into_iter()
            .map(|(name, value)| format!("{name}:{value};"))
            .collect(),
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

    /// A theme adroit comes with, as the work host reads it.
    fn preset(name: &str) -> Palette {
        let text = match name {
            "Nightfox" => include_str!("../../host/themes/Nightfox"),
            "Catppuccin Latte" => include_str!("../../host/themes/Catppuccin Latte"),
            _ => unreachable!("no preset {name}"),
        };
        Palette::from_ghostty(text).unwrap()
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
    fn until_a_theme_is_read_the_window_is_drawn_as_nightfox_draws_it() {
        let tokens = include_str!("../assets/css/tokens.css");
        let w = window(&Palette::default());
        for (name, value) in w.vars.iter().filter(|(n, _)| !n.starts_with("--ansi-")) {
            let declared = format!("{name}: {value};");
            assert!(tokens.contains(&declared), "{declared}");
        }
        assert!(tokens.contains("color-scheme: dark;"));
    }

    #[test]
    fn focus_is_drawn_from_the_theme_between_the_lines_and_the_quiet_text() {
        let lum = |hex: &str| {
            Rgb::parse(hex)
                .map(|c| c.r as i32 + c.g as i32 + c.b as i32)
                .unwrap()
        };
        for name in ["Nightfox", "Catppuccin Latte"] {
            let w = window(&preset(name));
            let from_panel = |v: &str| (lum(var(&w, v)) - lum(var(&w, "--panel"))).abs();
            let steps = [
                "--line-strong",
                "--focus-line",
                "--focus-border",
                "--text-3",
            ];
            for pair in steps.windows(2) {
                assert!(
                    from_panel(pair[0]) < from_panel(pair[1]),
                    "{name}: {pair:?}"
                );
            }
        }
    }

    #[test]
    fn a_light_theme_draws_its_lines_and_focus_closer_to_its_panels() {
        let lum = |hex: &str| {
            Rgb::parse(hex)
                .map(|c| c.r as f64 + c.g as f64 + c.b as f64)
                .unwrap()
        };
        let share = |w: &Window, v: &str| {
            let (panel, text) = (lum(var(w, "--panel")), lum(var(w, "--text")));
            (lum(var(w, v)) - panel) / (text - panel)
        };
        let (dark, light) = (
            window(&preset("Nightfox")),
            window(&preset("Catppuccin Latte")),
        );
        for v in ["--line", "--line-strong", "--focus-line", "--focus-border"] {
            assert!(share(&light, v) < share(&dark, v), "{v}");
        }
    }

    #[test]
    fn the_window_is_drawn_on_the_theme_s_background_in_its_foreground() {
        let p = preset("Nightfox");
        let w = window(&p);
        assert_eq!(var(&w, "--panel"), "#192330");
        assert_eq!(var(&w, "--text"), "#cdcecf");
        assert_eq!(var(&w, "--wait"), p.ansi[3].hex());
        assert_eq!(var(&w, "--fail"), p.ansi[1].hex());
        assert!(w.dark);
    }

    #[test]
    fn every_ansi_colour_of_the_theme_is_a_variable_of_its_number() {
        let p = preset("Nightfox");
        let w = window(&p);
        for (n, colour) in p.ansi.iter().enumerate() {
            assert_eq!(var(&w, ANSI[n]), colour.hex(), "{n}");
            assert_eq!(ANSI[n], format!("--ansi-{n}"));
        }
    }

    #[test]
    fn raised_parts_lean_toward_the_text_and_the_ground_away_from_it() {
        let lum = |hex: &str| {
            Rgb::parse(hex)
                .map(|c| c.r as u32 + c.g as u32 + c.b as u32)
                .unwrap()
        };
        for name in ["Nightfox", "Catppuccin Latte"] {
            let p = preset(name);
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
    fn a_light_theme_casts_fainter_shadows_and_its_key_caps_read_in_its_text() {
        let alpha = |w: &Window| {
            let s = var(w, "--shadow");
            s.trim_end_matches(')')
                .rsplit(',')
                .next()
                .unwrap()
                .parse::<f64>()
                .unwrap()
        };
        let (dark, light) = (
            window(&preset("Nightfox")),
            window(&preset("Catppuccin Latte")),
        );
        assert!(alpha(&light) * 6.0 <= alpha(&dark), "{light:?}");
        assert_eq!(var(&light, "--kbd-text"), var(&light, "--text-2"));
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

    fn token<'a>(d: &'a Document, name: &str) -> &'a str {
        let at = d.tokens.find(&format!("{name}:")).unwrap() + name.len() + 1;
        let rest = &d.tokens[at..];
        &rest[..rest.find(';').unwrap()]
    }

    #[test]
    fn a_document_is_painted_on_the_window_s_panel_in_its_text_and_lines() {
        for name in ["Nightfox", "Catppuccin Latte"] {
            let p = preset(name);
            let (w, d) = (window(&p), document(&p));
            assert_eq!(token(&d, "--bgColor-default"), var(&w, "--panel"), "{name}");
            assert_eq!(token(&d, "--fgColor-default"), var(&w, "--text"), "{name}");
            assert_eq!(token(&d, "--fgColor-muted"), var(&w, "--text-2"), "{name}");
            assert_eq!(
                token(&d, "--borderColor-default"),
                var(&w, "--line-strong"),
                "{name}"
            );
            assert_eq!(token(&d, "--bgColor-muted"), var(&w, "--raised"), "{name}");
            assert_eq!(d.scheme, if p.is_dark() { "dark" } else { "light" });
        }
    }

    #[test]
    fn links_alerts_and_code_take_the_theme_s_own_colours() {
        let p = preset("Nightfox");
        let d = document(&p);
        for (name, ansi) in [
            ("--fgColor-accent", 4),
            ("--fgColor-success", 2),
            ("--fgColor-attention", 3),
            ("--fgColor-danger", 1),
            ("--fgColor-done", 5),
            ("--prettylights-syntax-keyword", 5),
            ("--prettylights-syntax-string", 2),
            ("--prettylights-syntax-entity", 4),
            ("--prettylights-syntax-constant", 6),
            ("--prettylights-syntax-variable", 3),
        ] {
            assert_eq!(token(&d, name), p.ansi[ansi].hex(), "{name}");
        }
        assert_eq!(
            token(&d, "--prettylights-syntax-comment"),
            var(&window(&p), "--text-3")
        );
    }

    #[test]
    fn a_diagram_s_shapes_stand_out_from_its_block_in_a_light_theme_as_in_a_dark_one() {
        for name in ["Nightfox", "Catppuccin Latte"] {
            let p = preset(name);
            let (w, d) = (window(&p), document(&p));
            // The shapes are outlined in the quiet text, not the lines, which
            // a light theme draws too faint to outline anything.
            assert_eq!(token(&d, "--pele-border"), var(&w, "--text-3"), "{name}");
            assert_eq!(token(&d, "--pele-surface"), var(&w, "--panel"), "{name}");
            assert_eq!(token(&d, "--pele-accent"), p.ansi[4].hex(), "{name}");
        }
    }
}
