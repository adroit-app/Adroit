//! What a console holds, put in the shape the reader's Neovim takes: its
//! lines as text, their colours as marks over bytes, and where its cursor is.

use adroit_nvim::Mark;
use adroit_term::{Rgb, Screen};

/// The lines of `screen`, trailing blanks dropped, the stretches not drawn
/// in `foreground` on the console's own background marked with their look,
/// and the cursor as a line and a byte.
pub fn reading(screen: &Screen, foreground: Rgb) -> (Vec<String>, Vec<Mark>, (usize, usize)) {
    let mut lines = Vec::with_capacity(screen.lines.len());
    let mut marks = Vec::new();
    for (i, line) in screen.lines.iter().enumerate() {
        let full = line.text();
        let kept = full.trim_end().len();
        let mut at = 0;
        for run in &line.runs {
            let start = at;
            at += run.text.len();
            let end = at.min(kept);
            let s = &run.style;
            let plain =
                s.fg == foreground && s.bg.is_none() && !s.bold && !s.italic && !s.underline;
            if start >= end || plain {
                continue;
            }
            marks.push(Mark {
                line: i,
                start,
                end,
                fg: s.fg.hex(),
                bg: s.bg.map(Rgb::hex),
                bold: s.bold,
                italic: s.italic,
                underline: s.underline,
            });
        }
        lines.push(full[..kept].to_string());
    }
    let cursor = screen
        .cursor
        .map(|c| {
            let line = screen.lines.get(c.row);
            (c.row, line.map_or(0, |l| l.byte_at(c.col)))
        })
        .unwrap_or((screen.lines.len().saturating_sub(1), 0));
    (lines, marks, cursor)
}

/// Where the reader's cursor is among what it reads, and how many lines a
/// page of it is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Edge {
    /// The line the cursor is on, counted from 0.
    pub line: usize,
    /// The lines the reader holds.
    pub lines: usize,
    /// The lines the reader shows at once.
    pub page: usize,
}

/// How far `key` would move the reader past the first or last of the lines
/// it holds, in lines of the wheel: back when positive, forward when
/// negative. `None` when it moves within them, or not up or down.
pub fn past_edge(key: &adroit_term::Key, at: Edge) -> Option<i32> {
    if key.alt || key.meta {
        return None;
    }
    let half = (at.page / 2).max(1) as i32;
    let full = at.page.max(1) as i32;
    let by = match (key.ctrl, key.key.as_str()) {
        (false, "k" | "ArrowUp") | (true, "y") => 1,
        (true, "u") => half,
        (true, "b") => full,
        (false, "j" | "ArrowDown") | (true, "e") => -1,
        (true, "d") => -half,
        (true, "f") => -full,
        _ => return None,
    };
    let first = at.line == 0;
    let last = at.line + 1 >= at.lines;
    ((by > 0 && first) || (by < 0 && last)).then_some(by)
}

/// Claude Code's key for scrolling to its newest message.
pub fn newest() -> adroit_term::Key {
    adroit_term::Key::new("End").ctrl()
}

/// The key that scrolls the session to its end or its start, when `key`
/// after `previous` moves the reader to its last line, as `G` does, or to its
/// first, as `gg` does. `None` for any other key. The wheel would stop short
/// of a long session, as a terminal turns it only so far at once.
pub fn to_end(
    key: &adroit_term::Key,
    previous: Option<&adroit_term::Key>,
) -> Option<adroit_term::Key> {
    let plain = |k: &adroit_term::Key| !k.ctrl && !k.alt && !k.meta;
    if !plain(key) {
        return None;
    }
    match key.key.as_str() {
        "G" => Some(newest()),
        "g" if previous.is_some_and(|p| plain(p) && p.key == "g") => {
            Some(adroit_term::Key::new("Home").ctrl())
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use adroit_term::{Cursor, CursorShape, Line, Run, Style};

    use super::*;

    const FG: Rgb = Rgb {
        r: 200,
        g: 200,
        b: 200,
    };

    fn key(name: &str) -> adroit_term::Key {
        adroit_term::Key::new(name)
    }

    #[test]
    fn moving_up_from_the_first_line_scrolls_the_session_back_by_as_much() {
        let at_top = Edge {
            line: 0,
            lines: 40,
            page: 40,
        };
        assert_eq!(past_edge(&key("k"), at_top), Some(1));
        assert_eq!(past_edge(&key("ArrowUp"), at_top), Some(1));
        assert_eq!(past_edge(&key("y").ctrl(), at_top), Some(1));
        assert_eq!(past_edge(&key("u").ctrl(), at_top), Some(20));
        assert_eq!(past_edge(&key("b").ctrl(), at_top), Some(40));
    }

    #[test]
    fn moving_down_from_the_last_line_scrolls_the_session_forward() {
        let at_bottom = Edge {
            line: 39,
            lines: 40,
            page: 40,
        };
        assert_eq!(past_edge(&key("j"), at_bottom), Some(-1));
        assert_eq!(past_edge(&key("e").ctrl(), at_bottom), Some(-1));
        assert_eq!(past_edge(&key("d").ctrl(), at_bottom), Some(-20));
        assert_eq!(past_edge(&key("f").ctrl(), at_bottom), Some(-40));
    }

    #[test]
    fn a_move_that_stays_inside_the_lines_or_goes_sideways_is_the_reader_s_own() {
        let middle = Edge {
            line: 10,
            lines: 40,
            page: 40,
        };
        assert_eq!(past_edge(&key("k"), middle), None);
        assert_eq!(past_edge(&key("j"), middle), None);
        let at_top = Edge {
            line: 0,
            lines: 40,
            page: 40,
        };
        assert_eq!(past_edge(&key("j"), at_top), None);
        assert_eq!(past_edge(&key("l"), at_top), None);
        assert_eq!(past_edge(&key("k").meta(), at_top), None);
    }
    const RED: Rgb = Rgb { r: 255, g: 0, b: 0 };

    fn run(text: &str, fg: Rgb, wide: bool) -> Run {
        Run {
            text: text.into(),
            width: if wide { 2 } else { text.chars().count() },
            wide,
            apart: wide,
            style: Style {
                fg,
                bg: None,
                bold: false,
                italic: false,
                underline: false,
                strike: false,
            },
        }
    }

    fn screen(lines: Vec<Line>, cursor: Option<(usize, usize)>) -> Screen {
        Screen {
            cols: 20,
            rows: lines.len(),
            lines,
            cursor: cursor.map(|(row, col)| Cursor {
                row,
                col,
                shape: CursorShape::Block,
            }),
            background: Rgb { r: 0, g: 0, b: 0 },
        }
    }

    #[test]
    fn coloured_text_is_marked_by_its_bytes_and_plain_text_is_not() {
        let s = screen(
            vec![Line {
                runs: vec![
                    run("say ", FG, false),
                    run("日", RED, true),
                    run("ok   ", RED, false),
                ],
            }],
            None,
        );
        let (lines, marks, _) = reading(&s, FG);
        assert_eq!(lines, ["say 日ok"]);
        // 日 is three bytes; the blanks after "ok" are left out.
        assert_eq!(
            marks.iter().map(|m| (m.start, m.end)).collect::<Vec<_>>(),
            [(4, 7), (7, 9)]
        );
        assert_eq!(marks[0].fg, "#ff0000");
    }

    #[test]
    fn the_cursor_is_found_by_its_cell_among_wide_characters() {
        let s = screen(
            vec![
                Line {
                    runs: vec![run("top", FG, false)],
                },
                Line {
                    runs: vec![
                        run("日", FG, true),
                        run("本", FG, true),
                        run("x", FG, false),
                    ],
                },
            ],
            Some((1, 4)),
        );
        assert_eq!(reading(&s, FG).2, (1, 6));
        // A mark combined with the character before it takes no cell of its own.
        let marked = screen(
            vec![
                Line {
                    runs: vec![run("か\u{3099}", FG, true), run("x", FG, false)],
                },
                Line {
                    runs: vec![run("e\u{301}xy", FG, false)],
                },
            ],
            Some((0, 2)),
        );
        assert_eq!(reading(&marked, FG).2, (0, 6));
        let on_y = screen(marked.lines.clone(), Some((1, 2)));
        assert_eq!(reading(&on_y, FG).2, (1, 4));
        // Without a cursor shown, the reader starts at the last line.
        assert_eq!(reading(&screen(s.lines.clone(), None), FG).2, (1, 0));
    }

    #[test]
    fn going_to_the_last_or_the_first_line_scrolls_the_session_all_the_way() {
        let g = key("g");
        let big_g = key("G").shift();
        // Claude Code's own keys for its newest and its first message.
        assert_eq!(to_end(&big_g, None), Some(key("End").ctrl()));
        assert_eq!(to_end(&g, Some(&g)), Some(key("Home").ctrl()));
        // A lone g waits for the next key; one after another key is a new start.
        assert_eq!(to_end(&g, None), None);
        assert_eq!(to_end(&g, Some(&key("j"))), None);
        assert_eq!(to_end(&key("j"), Some(&g)), None);
        assert_eq!(to_end(&key("g").ctrl(), Some(&g)), None);
    }
}
