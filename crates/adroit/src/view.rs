//! Drawing a terminal-like screen from cells, shared by the session consoles,
//! the terminals and the prompt.

use adroit_term::{CursorShape, Picture, Screen};
use dioxus::prelude::*;

use crate::style::use_cell_height;

/// One screen, each cell exactly `cell_width` wide, so wide characters and the
/// cursor stay on the grid whatever the font's own metrics are.
#[component]
pub fn ScreenView(
    screen: Screen,
    cell_width: f64,
    show_cursor: bool,
    /// Images over the text, from the kitty graphics protocol.
    #[props(default)]
    pictures: Vec<Picture>,
) -> Element {
    let ch = use_cell_height();
    let cw = cell_width;
    let blinks =
        screen.cursor.is_some_and(|c| c.blink) && crate::settings::current().console.cursor_blink;
    let cursor = screen.cursor.filter(|_| show_cursor).map(|c| {
        let wide = screen.lines.get(c.row).is_some_and(|l| l.wide_at(c.col));
        let cells = if wide { 2.0 * cw } else { cw };
        let (w, h, top) = match c.shape {
            CursorShape::Block => (cells, ch, 0.0),
            CursorShape::Beam => (2.0, ch, 0.0),
            CursorShape::Underline => (cells, 2.0, ch - 2.0),
        };
        format!(
            "left:{}px;top:{}px;width:{w}px;height:{h}px",
            c.col as f64 * cw,
            c.row as f64 * ch + top
        )
    });
    rsx! {
        div { class: "screen",
            for (i , line) in screen.lines.iter().enumerate() {
                div { key: "{i}", class: "screen-line",
                    for (j , run) in line.runs.iter().enumerate() {
                        {
                            let mut style = run_style(run, cw, ch);
                            let drawn = run.apart.then(|| run.text.chars().next()).flatten().and_then(adroit_term::block);
                            if let Some((parts, alpha)) = &drawn {
                                style.push_str(&block_style(parts, *alpha, run.style.fg, run.style.bg, run.width as f64 * cw, ch));
                            }
                            // Keyed by its look as well: a kept span whose
                            // style changed went on showing the old one.
                            rsx! { span { key: "{j}{style}", style: "{style}", if drawn.is_none() { "{run.text}" } } }
                        }
                    }
                }
            }
            for (i , p) in pictures.iter().enumerate() {
                {
                    let [x, y, w, h] = p.dst;
                    let [sx, sy, sw, sh] = p.src;
                    // The image scaled so the part shown fills its place.
                    let (kx, ky) = (f64::from(w) * cw / f64::from(sw), f64::from(h) * ch / f64::from(sh));
                    rsx! { div {
                        key: "p{i}",
                        class: "screen-picture",
                        style: "left:{f64::from(x) * cw}px;top:{f64::from(y) * ch}px;width:{f64::from(w) * cw}px;height:{f64::from(h) * ch}px",
                        img {
                            src: "{p.url}",
                            style: "left:{-f64::from(sx) * kx}px;top:{-f64::from(sy) * ky}px;width:{f64::from(p.size[0]) * kx}px;height:{f64::from(p.size[1]) * ky}px",
                        }
                    } }
                }
            }
            if let Some(style) = cursor {
                div { class: if blinks { "screen-cursor blink" } else { "screen-cursor" }, style: "{style}" }
            }
        }
    }
}

/// How a run is drawn in cells `cw` wide and `ch` tall.
fn run_style(run: &adroit_term::Run, cw: f64, ch: f64) -> String {
    let s = &run.style;
    let mut out = format!("width:{}px;color:{}", run.width as f64 * cw, s.fg.hex());
    // A wide character comes from a fallback font whose glyph is an em
    // wide, narrower than two cells of a monospace font; drawn larger, it
    // fills them, as terminals size their fallback fonts.
    if run.wide {
        out.push_str(&format!(";font-size:{}px", (2.0 * cw).min(ch)));
    }
    if let Some(bg) = s.bg {
        out.push_str(&format!(";background:{}", bg.hex()));
    }
    if s.bold {
        out.push_str(";font-weight:600");
    }
    if s.italic {
        out.push_str(";font-style:italic");
    }
    match (s.underline, s.strike) {
        (true, true) => out.push_str(";text-decoration:underline line-through"),
        (true, false) => out.push_str(";text-decoration:underline"),
        (false, true) => out.push_str(";text-decoration:line-through"),
        _ => {}
    }
    out
}

/// Keys as the bindings write them, a cap for each key of the sequence.
#[component]
pub fn Keys(keys: String) -> Element {
    rsx! {
        for (i , chord) in keys.split(' ').map(str::to_string).enumerate() {
            kbd { key: "{i}", "{chord}" }
        }
    }
}

/// The background that paints a block element's `parts` of a cell `width`
/// pixels wide and `height` tall in `fg`, `alpha` strong, in place of the font's glyph.
fn block_style(
    parts: &[adroit_term::Part],
    alpha: f32,
    fg: adroit_term::Rgb,
    bg: Option<adroit_term::Rgb>,
    width: f64,
    height: f64,
) -> String {
    let colour = format!("rgba({},{},{},{alpha})", fg.r, fg.g, fg.b);
    let layers: Vec<String> = parts
        .iter()
        .map(|&(l, t, r, b)| {
            let (x, y) = (l as f64 * width, t as f64 * height);
            let (w, h) = ((r - l) as f64 * width, (b - t) as f64 * height);
            format!("linear-gradient({colour},{colour}) {x}px {y}px / {w}px {h}px no-repeat")
        })
        .collect();
    // The shorthand sets every layer, so the cell's own colour goes last.
    let under = bg.map(|c| format!(",{}", c.hex())).unwrap_or_default();
    format!(
        ";background:{}{under};height:{}px",
        layers.join(","),
        height
    )
}

#[cfg(test)]
mod tests {
    use adroit_term::{Rgb, Run, Style};

    use super::*;

    fn run(text: &str, width: usize, wide: bool) -> Run {
        Run {
            text: text.into(),
            width,
            wide,
            apart: wide,
            style: Style {
                fg: Rgb::new(1, 2, 3),
                bg: None,
                bold: false,
                italic: false,
                underline: false,
                strike: false,
            },
        }
    }

    #[test]
    fn a_wide_character_is_drawn_as_large_as_its_two_cells() {
        let style = run_style(&run("日", 2, true), 8.0, 18.0);
        assert!(style.contains("width:16px"), "{style}");
        assert!(style.contains("font-size:16px"), "{style}");
    }

    #[test]
    fn a_wide_character_never_grows_taller_than_its_cell() {
        let style = run_style(&run("日", 2, true), 10.0, 17.0);
        assert!(style.contains("font-size:17px"), "{style}");
    }

    #[test]
    fn narrow_text_keeps_the_console_s_size() {
        let style = run_style(&run("ab", 2, false), 8.0, 18.0);
        assert!(!style.contains("font-size"), "{style}");
    }
}
