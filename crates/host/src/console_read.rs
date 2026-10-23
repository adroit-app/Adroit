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

#[cfg(test)]
mod tests {
    use adroit_term::{Cursor, CursorShape, Line, Run, Style};

    use super::*;

    const FG: Rgb = Rgb {
        r: 200,
        g: 200,
        b: 200,
    };

    #[tokio::test]
    async fn read_mode_can_move_above_the_live_screen_into_terminal_scrollback() {
        use std::os::unix::fs::PermissionsExt;
        use std::time::Duration;

        use adroit_nvim::Embedded;
        use adroit_term::{Daemon, Palette, Spawn};

        let dir = tempfile::Builder::new()
            .prefix("adroit-read-")
            .tempdir_in("/tmp")
            .unwrap();
        std::fs::set_permissions(dir.path(), std::fs::Permissions::from_mode(0o700)).unwrap();
        let socket = dir.path().join("d.sock");
        let task = crate::native_test_daemon::serve_process(socket.clone());
        let daemon = Daemon::new(socket);
        tokio::time::timeout(Duration::from_secs(5), async {
            while daemon.sessions().await.is_err() {
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        })
        .await
        .unwrap();
        let id = daemon
            .spawn(Spawn {
                program: "/bin/sh".into(),
                args: vec![
                    "-c".into(),
                    "i=1; while [ $i -le 40 ]; do echo line$i; i=$((i + 1)); done; printf ready; sleep 30".into(),
                ],
                cwd: dir.path().into(),
                cols: 40,
                rows: 6,
                ..Spawn::default()
            })
            .await
            .unwrap();
        tokio::time::timeout(Duration::from_secs(5), async {
            loop {
                let visible = daemon.visible(id).await.unwrap();
                if visible.iter().any(|line| line == "ready") {
                    assert!(!visible.iter().any(|line| line == "line1"));
                    break;
                }
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        })
        .await
        .unwrap();

        let palette = Palette::default();
        let all = daemon.everything(id, 40, &palette).await.unwrap();
        let (lines, marks, cursor) = reading(&all, palette.foreground);
        assert_eq!(lines.first().map(String::as_str), Some("line1"));
        assert_eq!(lines.last().map(String::as_str), Some("ready"));
        let reader = Embedded::spawn_reader("nvim", &["--clean"], 40, 6, |_| {})
            .await
            .unwrap();
        reader
            .show(&lines, &marks, cursor, true, &all.background.hex())
            .await
            .unwrap();
        reader.input("<C-u><C-u>").await.unwrap();
        tokio::time::timeout(Duration::from_secs(5), async {
            loop {
                let above_live_screen = reader
                    .state()
                    .screen()
                    .text()
                    .first()
                    .and_then(|line| line.strip_prefix("line"))
                    .and_then(|number| number.parse::<usize>().ok())
                    .is_some_and(|number| number < 36);
                if above_live_screen {
                    break;
                }
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        })
        .await
        .unwrap();
        reader.input("gg").await.unwrap();
        tokio::time::timeout(Duration::from_secs(5), async {
            while reader.state().screen().text().first().map(String::as_str) != Some("line1") {
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        })
        .await
        .unwrap();
        assert_eq!(reader.state().screen().cursor.map(|c| c.row), Some(0));
        reader.input("G").await.unwrap();
        tokio::time::timeout(Duration::from_secs(5), async {
            while !reader
                .state()
                .screen()
                .text()
                .iter()
                .any(|line| line == "ready")
            {
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        })
        .await
        .unwrap();
        daemon.end(id).await.unwrap();
        task.abort();
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
                blink: false,
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
}
