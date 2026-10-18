use std::collections::HashMap;

use felis_grid::{AttrFlags, Cell, Color, Grapheme, ScreenBuffer};
use felis_protocol::kitty_graphics::placeholder::PLACEHOLDER;
use felis_protocol::messages::CursorStyle;

use crate::{Palette, Rgb};

/// What a terminal shows, row by row.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Screen {
    pub cols: usize,
    pub rows: usize,
    pub lines: Vec<Line>,
    pub cursor: Option<Cursor>,
    pub background: Rgb,
}

/// One row as runs of equally styled text.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Line {
    pub runs: Vec<Run>,
}

/// Whether a narrow character is drawn in a cell of its own: half-width
/// katakana and hangul, which monospace fonts leave to a fallback of another
/// width, and block elements, which are drawn as the cell's own boxes.
pub fn set_apart(c: char) -> bool {
    matches!(c, '\u{FF61}'..='\u{FFDC}') || block(c).is_some()
}

/// A part of a cell, as fractions of its width and height: left, top, right,
/// bottom.
pub type Part = (f32, f32, f32, f32);

/// A block element as the parts of its cell it fills and how strongly, as a
/// terminal draws them: a font's glyphs stop short of the cell, and blocks
/// stacked into a picture, as Claude Code's banner is, show seams.
pub fn block(c: char) -> Option<(Vec<Part>, f32)> {
    let eighth = |n: u32| n as f32 / 8.0;
    let q = |ul: bool, ur: bool, ll: bool, lr: bool| {
        let all = [
            (ul, (0.0, 0.0, 0.5, 0.5)),
            (ur, (0.5, 0.0, 1.0, 0.5)),
            (ll, (0.0, 0.5, 0.5, 1.0)),
            (lr, (0.5, 0.5, 1.0, 1.0)),
        ];
        all.into_iter()
            .filter(|(on, _)| *on)
            .map(|(_, p)| p)
            .collect()
    };
    let parts: Vec<Part> = match c as u32 {
        0x2580 => vec![(0.0, 0.0, 1.0, 0.5)],
        // Lower one eighth to the full block.
        n @ 0x2581..=0x2588 => vec![(0.0, 1.0 - eighth(n - 0x2580), 1.0, 1.0)],
        // Left seven eighths to one eighth.
        n @ 0x2589..=0x258F => vec![(0.0, 0.0, eighth(0x2590 - n), 1.0)],
        0x2590 => vec![(0.5, 0.0, 1.0, 1.0)],
        n @ 0x2591..=0x2593 => {
            return Some((vec![(0.0, 0.0, 1.0, 1.0)], (n - 0x2590) as f32 / 4.0));
        }
        0x2594 => vec![(0.0, 0.0, 1.0, eighth(1))],
        0x2595 => vec![(1.0 - eighth(1), 0.0, 1.0, 1.0)],
        0x2596 => q(false, false, true, false),
        0x2597 => q(false, false, false, true),
        0x2598 => q(true, false, false, false),
        0x2599 => q(true, false, true, true),
        0x259A => q(true, false, false, true),
        0x259B => q(true, true, true, false),
        0x259C => q(true, true, false, true),
        0x259D => q(false, true, false, false),
        0x259E => q(false, true, true, false),
        0x259F => q(false, true, true, true),
        _ => return None,
    };
    Some((parts, 1.0))
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Run {
    pub text: String,
    /// The columns the run covers; a wide character counts two.
    pub width: usize,
    /// The run is one wide character, with any marks combined with it. Each
    /// wide character has a run of its own so it can be drawn in exactly two
    /// cells, whatever width the font gives it.
    pub wide: bool,
    /// The run is one character drawn in cells of its own: a wide one, or a
    /// narrow one the console's font has no glyph for, whose fallback would
    /// be wider or narrower than a cell and push what follows aside.
    pub apart: bool,
    pub style: Style,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Style {
    pub fg: Rgb,
    /// `None` when the cell shows the terminal's own background.
    pub bg: Option<Rgb>,
    pub bold: bool,
    pub italic: bool,
    pub underline: bool,
    pub strike: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Cursor {
    pub row: usize,
    pub col: usize,
    pub shape: CursorShape,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CursorShape {
    Block,
    Beam,
    Underline,
}

impl Line {
    pub fn text(&self) -> String {
        self.runs.iter().map(|r| r.text.as_str()).collect()
    }

    /// Whether a wide character starts at `col`.
    pub fn wide_at(&self, col: usize) -> bool {
        let mut at = 0;
        for run in &self.runs {
            if at >= col {
                return at == col && run.wide;
            }
            at += run.width;
        }
        false
    }

    /// The byte of the line's text where the cell `col` starts, or the end
    /// of the text past its last cell.
    pub fn byte_at(&self, col: usize) -> usize {
        let mut cells = 0;
        let mut bytes = 0;
        for run in &self.runs {
            if run.wide {
                if cells >= col {
                    return bytes;
                }
                cells += run.width;
                bytes += run.text.len();
                continue;
            }
            for (i, ch) in run.text.char_indices() {
                // A mark combined with the character before it shares its cell.
                if i > 0 && felis_grid::char_cell_width(ch) == 0 {
                    continue;
                }
                if cells >= col {
                    return bytes + i;
                }
                cells += 1;
            }
            bytes += run.text.len();
        }
        bytes
    }
}

impl Screen {
    /// The visible text, one string per row, without trailing blanks.
    pub fn text(&self) -> Vec<String> {
        self.lines
            .iter()
            .map(|l| l.text().trim_end().to_string())
            .collect()
    }
}

/// The colours a program set over the palette, with OSC 4, 10 and 11.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct Overrides {
    pub indexed: HashMap<u8, Rgb>,
    pub foreground: Option<Rgb>,
    pub background: Option<Rgb>,
    /// The program turned the whole screen to reverse video.
    pub reverse: bool,
}

/// The colours a screen is drawn in: the palette, with what the program
/// set over it.
struct Colours {
    indexed: [Rgb; 256],
    foreground: Rgb,
    background: Rgb,
    reverse: bool,
}

impl Colours {
    fn new(palette: &Palette, overrides: &Overrides) -> Self {
        let mut indexed = [Rgb::new(0, 0, 0); 256];
        for (i, c) in indexed.iter_mut().enumerate() {
            *c = overrides
                .indexed
                .get(&(i as u8))
                .copied()
                .unwrap_or_else(|| palette.indexed(i as u8));
        }
        Self {
            indexed,
            foreground: overrides.foreground.unwrap_or(palette.foreground),
            background: overrides.background.unwrap_or(palette.background),
            reverse: overrides.reverse,
        }
    }

    fn resolve(&self, c: Color) -> Option<Rgb> {
        match c {
            Color::Default => None,
            Color::Indexed(i) => Some(self.indexed[usize::from(i)]),
            Color::Rgb(r, g, b) => Some(Rgb::new(r, g, b)),
        }
    }

    fn style(&self, attrs: &felis_grid::Attributes) -> Style {
        let f = attrs.flags;
        let mut fg = self.resolve(attrs.fg).unwrap_or(self.foreground);
        if f.contains(AttrFlags::FAINT) {
            let dim = |v: u8| (u16::from(v) * 2 / 3) as u8;
            fg = Rgb::new(dim(fg.r), dim(fg.g), dim(fg.b));
        }
        let mut bg = self.resolve(attrs.bg);
        if f.contains(AttrFlags::REVERSE) != self.reverse {
            let back = bg.unwrap_or(self.background);
            bg = Some(fg);
            fg = back;
        }
        Style {
            fg,
            bg,
            bold: f.contains(AttrFlags::BOLD),
            italic: f.contains(AttrFlags::ITALIC),
            underline: f.contains(AttrFlags::UNDERLINE),
            strike: f.contains(AttrFlags::STRIKETHROUGH),
        }
    }
}

/// What `buffer` shows, in `palette` with the program's `overrides` over
/// it; the cursor only when `cursor` is asked for, as a buffer read from
/// scrollback has none of its own.
pub(crate) fn screen_of(
    buffer: &ScreenBuffer,
    palette: &Palette,
    overrides: &Overrides,
    cursor: bool,
) -> Screen {
    let colours = Colours::new(palette, overrides);
    let cols = usize::from(buffer.cols());
    let lines = (0..buffer.rows())
        .map(|r| line_of(buffer, buffer.row_content(r).unwrap_or(&[]), cols, &colours))
        .collect();
    let c = buffer.cursor();
    Screen {
        cols,
        rows: usize::from(buffer.rows()),
        lines,
        cursor: (cursor && c.visible).then(|| Cursor {
            row: usize::from(c.row),
            col: usize::from(c.col),
            shape: match buffer.cursor_style() {
                CursorStyle::Underline => CursorShape::Underline,
                CursorStyle::Bar => CursorShape::Beam,
                CursorStyle::Block => CursorShape::Block,
            },
        }),
        background: colours.background,
    }
}

/// One row of cells as runs of equally styled text, padded to `cols`.
fn line_of(buffer: &ScreenBuffer, cells: &[Cell], cols: usize, colours: &Colours) -> Line {
    let at = |i: usize| cells.get(i).copied().unwrap_or(Cell::BLANK);
    let mut out = Line::default();
    let mut col = 0;
    while col < cols {
        let cell = at(col);
        let text = match cell.grapheme {
            Grapheme::Spacer | Grapheme::SizedSpacer => {
                col += 1;
                continue;
            }
            Grapheme::Empty => " ".to_string(),
            Grapheme::Ascii(b) => char::from(b).to_string(),
            Grapheme::Char(c) => c.to_string(),
            Grapheme::Cluster(id) => buffer.cluster_str(id).unwrap_or(" ").to_string(),
        };
        let wide = col + 1 < cols && matches!(at(col + 1).grapheme, Grapheme::Spacer);
        let width = if wide { 2 } else { 1 };
        let attrs = buffer.style(cell.style);
        // A placeholder cell is where an image is drawn; the image covers
        // it, and its character has no glyph of its own.
        let hidden = attrs.flags.contains(AttrFlags::CONCEAL) || text.starts_with(PLACEHOLDER);
        let text = if hidden {
            " ".repeat(text.chars().count().max(1))
        } else {
            text
        };
        let style = colours.style(attrs);
        let first = text.chars().next().unwrap_or(' ');
        let apart = wide || set_apart(first);
        match out.runs.last_mut() {
            Some(last) if last.style == style && !last.apart && !apart => {
                last.text.push_str(&text);
                last.width += width;
            }
            _ => out.runs.push(Run {
                text,
                width,
                wide,
                apart,
                style,
            }),
        }
        col += width;
    }
    out
}

/// The screen a terminal of `cols` columns draws `bytes` on, each line of
/// them on a row of its own: how lines read back from a session with their
/// colours become a screen, in `palette` with the program's `overrides`.
pub(crate) fn screen_of_lines(
    bytes: &[u8],
    cols: u16,
    palette: &Palette,
    overrides: &Overrides,
) -> Screen {
    let rows = bytes.split(|&b| b == b'\n').count().max(1);
    let rows = u16::try_from(rows).unwrap_or(u16::MAX);
    let mut grid = felis_grid::Grid::with_scrollback(rows, cols.max(1), 0);
    let mut parser = felis_vt::Parser::new();
    for (i, line) in bytes.split(|&b| b == b'\n').enumerate() {
        if i > 0 {
            parser.advance(&mut grid, b"\r\n");
        }
        parser.advance(&mut grid, line);
        // Each line starts afresh, as each was read on its own.
        parser.advance(&mut grid, b"\x1b[0m");
    }
    screen_of(grid.screen(), palette, overrides, false)
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    pub(crate) fn screen(cols: usize, rows: usize, bytes: &[u8]) -> Screen {
        let mut grid = felis_grid::Grid::with_scrollback(rows as u16, cols as u16, 0);
        felis_vt::Parser::new().advance(&mut grid, bytes);
        let overrides = Overrides {
            reverse: grid.reverse_video(),
            ..Overrides::default()
        };
        screen_of(grid.screen(), &Palette::default(), &overrides, true)
    }

    #[test]
    fn lines_read_back_with_their_colours_keep_them_each_on_a_row() {
        let s = screen_of_lines(
            b"a\x1b[31mb\nc",
            4,
            &Palette::default(),
            &Overrides::default(),
        );
        assert_eq!(s.text(), vec!["ab", "c"]);
        assert_eq!(s.lines[0].runs[1].style.fg, Palette::default().ansi[1]);
        assert_eq!(s.lines[1].runs[0].style.fg, Palette::default().foreground);
        assert_eq!(s.cursor, None);
    }

    #[test]
    fn lines_read_back_are_in_the_colours_the_program_set() {
        let overrides = Overrides {
            indexed: HashMap::from([(1, Rgb::new(0, 0, 255))]),
            foreground: Some(Rgb::new(1, 2, 3)),
            background: Some(Rgb::new(4, 5, 6)),
            reverse: false,
        };
        let s = screen_of_lines(b"a\x1b[31mb", 4, &Palette::default(), &overrides);
        assert_eq!(s.lines[0].runs[0].style.fg, Rgb::new(1, 2, 3));
        assert_eq!(s.lines[0].runs[1].style.fg, Rgb::new(0, 0, 255));
        assert_eq!(s.background, Rgb::new(4, 5, 6));
    }

    #[test]
    fn a_colour_the_program_set_wins_over_the_palette() {
        let mut grid = felis_grid::Grid::with_scrollback(1, 4, 0);
        felis_vt::Parser::new().advance(&mut grid, b"\x1b[31mx");
        let overrides = Overrides {
            indexed: HashMap::from([(1, Rgb::new(1, 2, 3))]),
            ..Overrides::default()
        };
        let s = screen_of(grid.screen(), &Palette::default(), &overrides, true);
        assert_eq!(s.lines[0].runs[0].style.fg, Rgb::new(1, 2, 3));
    }

    #[test]
    fn plain_text_is_one_run_per_row() {
        let s = screen(10, 2, b"hello");
        assert_eq!(s.text(), vec!["hello", ""]);
        assert_eq!(s.lines[0].runs.len(), 1);
        assert_eq!(s.lines[0].runs[0].width, 10);
    }

    #[test]
    fn a_colour_change_starts_a_new_run() {
        let s = screen(10, 1, b"a\x1b[31mb\x1b[0mc");
        let runs = &s.lines[0].runs;
        assert_eq!(runs[0].text, "a");
        assert_eq!(runs[1].text, "b");
        assert_eq!(runs[1].style.fg, Palette::default().ansi[1]);
        assert!(runs[2].text.starts_with('c'));
    }

    #[test]
    fn each_wide_character_covers_two_columns_in_a_run_of_its_own() {
        let s = screen(6, 1, "日本".as_bytes());
        assert_eq!(s.text(), vec!["日本"]);
        let runs = &s.lines[0].runs;
        assert_eq!((runs[0].text.as_str(), runs[0].width), ("日", 2));
        assert_eq!((runs[1].text.as_str(), runs[1].width), ("本", 2));
    }

    #[test]
    fn a_column_knows_whether_a_wide_character_starts_there() {
        let s = screen(6, 1, "a日b".as_bytes());
        let line = &s.lines[0];
        assert!(!line.wide_at(0));
        assert!(line.wide_at(1));
        assert!(!line.wide_at(3));
        assert!(!line.wide_at(5));
    }

    #[test]
    fn a_mark_combined_with_a_wide_character_stays_in_its_cell() {
        let s = screen(6, 1, "か\u{3099}x".as_bytes());
        let runs = &s.lines[0].runs;
        assert_eq!((runs[0].text.as_str(), runs[0].width), ("か\u{3099}", 2));
        assert!(runs[1].text.starts_with('x'));
    }

    #[test]
    fn a_column_is_found_by_its_byte_past_marks_combined_with_a_character() {
        let s = screen(8, 2, "か\u{3099}x\r\ne\u{301}xy".as_bytes());
        assert_eq!(s.lines[0].byte_at(0), 0);
        assert_eq!(s.lines[0].byte_at(2), 6);
        assert_eq!(s.lines[1].byte_at(1), 3);
        assert_eq!(s.lines[1].byte_at(2), 4);
        // Past the text, the end of the line's text.
        assert_eq!(s.lines[1].byte_at(20), s.lines[1].text().len());
    }

    #[test]
    fn a_block_element_fills_the_parts_of_its_cell_it_names() {
        assert_eq!(block('█'), Some((vec![(0.0, 0.0, 1.0, 1.0)], 1.0)));
        assert_eq!(block('▀'), Some((vec![(0.0, 0.0, 1.0, 0.5)], 1.0)));
        assert_eq!(block('▄'), Some((vec![(0.0, 0.5, 1.0, 1.0)], 1.0)));
        assert_eq!(block('▌'), Some((vec![(0.0, 0.0, 0.5, 1.0)], 1.0)));
        assert_eq!(block('▐'), Some((vec![(0.5, 0.0, 1.0, 1.0)], 1.0)));
        assert_eq!(block('▛').map(|(p, _)| p.len()), Some(3));
        assert_eq!(block('▒').map(|(_, a)| a), Some(0.5));
        assert_eq!(block('a'), None);
        assert!(set_apart('█') && !set_apart('a'));
    }

    #[test]
    fn half_width_katakana_is_drawn_a_cell_at_a_time() {
        let s = screen(8, 1, "aｱｲb".as_bytes());
        let runs: Vec<(&str, usize, bool)> = s.lines[0]
            .runs
            .iter()
            .map(|r| (r.text.as_str(), r.width, r.apart))
            .take(4)
            .collect();
        assert_eq!(
            runs,
            [
                ("a", 1, false),
                ("ｱ", 1, true),
                ("ｲ", 1, true),
                ("b    ", 8 - 3, false)
            ]
        );
    }

    #[test]
    fn wide_characters_are_kept_apart_from_narrow_ones() {
        let s = screen(8, 1, "a日本b".as_bytes());
        let runs = &s.lines[0].runs;
        assert_eq!((runs[0].text.as_str(), runs[0].wide), ("a", false));
        assert_eq!(
            (runs[1].text.as_str(), runs[1].width, runs[1].wide),
            ("日", 2, true)
        );
        assert!(runs[3].text.starts_with('b') && !runs[3].wide);
    }

    #[test]
    fn truecolour_is_kept_exactly() {
        let s = screen(4, 1, b"\x1b[38;2;1;2;3mx");
        assert_eq!(s.lines[0].runs[0].style.fg, Rgb::new(1, 2, 3));
    }

    #[test]
    fn the_default_background_is_left_to_the_pane() {
        let s = screen(4, 1, b"x");
        assert_eq!(s.lines[0].runs[0].style.bg, None);
    }

    #[test]
    fn inverse_swaps_the_colours() {
        let p = Palette::default();
        let s = screen(4, 1, b"\x1b[7mx");
        let style = s.lines[0].runs[0].style;
        assert_eq!(style.fg, p.background);
        assert_eq!(style.bg, Some(p.foreground));
    }

    #[test]
    fn bold_italic_underline_and_strike_are_reported() {
        let s = screen(4, 1, b"\x1b[1;3;4;9mx");
        let st = s.lines[0].runs[0].style;
        assert!(st.bold && st.italic && st.underline && st.strike);
    }

    #[test]
    fn the_cursor_is_where_the_program_left_it() {
        let s = screen(10, 3, b"ab\r\ncd");
        let c = s.cursor.unwrap();
        assert_eq!((c.row, c.col), (1, 2));
        assert_eq!(c.shape, CursorShape::Block);
    }

    #[test]
    fn a_hidden_cursor_is_not_reported() {
        let s = screen(10, 1, b"\x1b[?25l");
        assert_eq!(s.cursor, None);
    }

    #[test]
    fn a_beam_cursor_is_reported_as_a_beam() {
        let s = screen(10, 1, b"\x1b[6 q");
        assert_eq!(s.cursor.unwrap().shape, CursorShape::Beam);
    }
}
