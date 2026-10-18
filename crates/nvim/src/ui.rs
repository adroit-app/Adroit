use std::collections::HashMap;

use adroit_term::{Cursor, CursorShape, Line, Rgb, Run, Screen, Style, set_apart};
use rmpv::Value;

/// What Neovim's UI protocol has drawn so far.
#[derive(Debug, Clone)]
pub struct UiState {
    cells: Vec<Vec<Cell>>,
    hl: HashMap<u64, Hl>,
    fg: Rgb,
    bg: Rgb,
    cursor: (usize, usize),
    modes: Vec<CursorShape>,
    mode: usize,
    mode_name: String,
    popup: Option<Popup>,
    cmdline: Option<Cmdline>,
    message: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct Cell {
    text: String,
    hl: u64,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
struct Hl {
    fg: Option<Rgb>,
    bg: Option<Rgb>,
    bold: bool,
    italic: bool,
    underline: bool,
    strike: bool,
    reverse: bool,
}

/// The completion menu, anchored at a grid cell.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Popup {
    pub items: Vec<PopupItem>,
    pub selected: Option<usize>,
    pub row: usize,
    pub col: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PopupItem {
    pub word: String,
    pub kind: String,
    pub menu: String,
    pub info: String,
}

/// The command line being typed, such as `:w` or `/pattern`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Cmdline {
    pub firstc: String,
    pub text: String,
    pub pos: usize,
}

impl Default for UiState {
    fn default() -> Self {
        Self {
            cells: Vec::new(),
            hl: HashMap::new(),
            fg: Rgb::new(0xe7, 0xe8, 0xeb),
            bg: Rgb::new(0x1d, 0x21, 0x26),
            cursor: (0, 0),
            modes: Vec::new(),
            mode: 0,
            mode_name: "normal".into(),
            popup: None,
            cmdline: None,
            message: None,
        }
    }
}

fn int(v: &Value) -> Option<i64> {
    v.as_i64()
}

fn uint(v: &Value) -> usize {
    v.as_u64().unwrap_or(0) as usize
}

fn text(v: &Value) -> String {
    v.as_str().unwrap_or_default().to_string()
}

fn rgb(v: &Value) -> Option<Rgb> {
    let n = int(v)?;
    (n >= 0).then(|| Rgb::new((n >> 16) as u8, (n >> 8) as u8, n as u8))
}

fn map_get<'a>(map: &'a Value, key: &str) -> Option<&'a Value> {
    map.as_map()?
        .iter()
        .find(|(k, _)| k.as_str() == Some(key))
        .map(|(_, v)| v)
}

fn flag(map: &Value, key: &str) -> bool {
    map_get(map, key).and_then(Value::as_bool).unwrap_or(false)
}

/// The concatenated text of a content list `[[attrs, text], ...]`.
fn content_text(v: &Value) -> String {
    v.as_array()
        .map(|chunks| {
            chunks
                .iter()
                .filter_map(|c| c.as_array().and_then(|a| a.get(1)).map(text))
                .collect()
        })
        .unwrap_or_default()
}

impl UiState {
    /// Apply the arguments of one `redraw` notification.
    pub fn apply(&mut self, redraw: &[Value]) {
        for batch in redraw {
            let Some(items) = batch.as_array() else {
                continue;
            };
            let Some(name) = items.first().and_then(Value::as_str) else {
                continue;
            };
            for args in &items[1..] {
                let args = args.as_array().map(Vec::as_slice).unwrap_or(&[]);
                self.event(name, args);
            }
        }
    }

    fn event(&mut self, name: &str, a: &[Value]) {
        match name {
            "grid_resize" if a.len() >= 3 => self.resize(uint(&a[1]), uint(&a[2])),
            "grid_clear" => {
                for row in &mut self.cells {
                    for cell in row {
                        *cell = Cell {
                            text: " ".into(),
                            hl: 0,
                        };
                    }
                }
            }
            "grid_cursor_goto" if a.len() >= 3 => self.cursor = (uint(&a[1]), uint(&a[2])),
            "grid_line" if a.len() >= 4 => self.line(uint(&a[1]), uint(&a[2]), &a[3]),
            "grid_scroll" if a.len() >= 7 => self.scroll(
                uint(&a[1]),
                uint(&a[2]),
                uint(&a[3]),
                uint(&a[4]),
                int(&a[5]).unwrap_or(0),
            ),
            "default_colors_set" if a.len() >= 2 => {
                if let Some(fg) = rgb(&a[0]) {
                    self.fg = fg;
                }
                if let Some(bg) = rgb(&a[1]) {
                    self.bg = bg;
                }
            }
            "hl_attr_define" if a.len() >= 2 => {
                let attrs = &a[1];
                self.hl.insert(
                    a[0].as_u64().unwrap_or(0),
                    Hl {
                        fg: map_get(attrs, "foreground").and_then(rgb),
                        bg: map_get(attrs, "background").and_then(rgb),
                        bold: flag(attrs, "bold"),
                        italic: flag(attrs, "italic"),
                        underline: flag(attrs, "underline")
                            || flag(attrs, "undercurl")
                            || flag(attrs, "underdouble"),
                        strike: flag(attrs, "strikethrough"),
                        reverse: flag(attrs, "reverse"),
                    },
                );
            }
            "mode_info_set" if a.len() >= 2 => {
                self.modes = a[1]
                    .as_array()
                    .map(|infos| {
                        infos
                            .iter()
                            .map(|info| {
                                match map_get(info, "cursor_shape").and_then(Value::as_str) {
                                    Some("vertical") => CursorShape::Beam,
                                    Some("horizontal") => CursorShape::Underline,
                                    _ => CursorShape::Block,
                                }
                            })
                            .collect()
                    })
                    .unwrap_or_default();
            }
            "mode_change" if a.len() >= 2 => {
                self.mode_name = text(&a[0]);
                self.mode = uint(&a[1]);
            }
            "popupmenu_show" if a.len() >= 4 => {
                let items = a[0]
                    .as_array()
                    .map(|items| {
                        items
                            .iter()
                            .map(|i| {
                                let f = i.as_array().map(Vec::as_slice).unwrap_or(&[]);
                                let at = |n: usize| f.get(n).map(text).unwrap_or_default();
                                PopupItem {
                                    word: at(0),
                                    kind: at(1),
                                    menu: at(2),
                                    info: at(3),
                                }
                            })
                            .collect()
                    })
                    .unwrap_or_default();
                let selected = int(&a[1]).filter(|n| *n >= 0).map(|n| n as usize);
                self.popup = Some(Popup {
                    items,
                    selected,
                    row: uint(&a[2]),
                    col: uint(&a[3]),
                });
            }
            "popupmenu_select" if !a.is_empty() => {
                if let Some(p) = &mut self.popup {
                    p.selected = int(&a[0]).filter(|n| *n >= 0).map(|n| n as usize);
                }
            }
            "popupmenu_hide" => self.popup = None,
            "cmdline_show" if a.len() >= 3 => {
                self.cmdline = Some(Cmdline {
                    text: content_text(&a[0]),
                    pos: uint(&a[1]),
                    firstc: text(&a[2]),
                });
            }
            "cmdline_pos" if !a.is_empty() => {
                if let Some(c) = &mut self.cmdline {
                    c.pos = uint(&a[0]);
                }
            }
            "cmdline_hide" => self.cmdline = None,
            "msg_show" if a.len() >= 2 => {
                let t = content_text(&a[1]);
                self.message = (!t.trim().is_empty()).then_some(t);
            }
            "msg_clear" => self.message = None,
            _ => {}
        }
    }

    fn resize(&mut self, cols: usize, rows: usize) {
        self.cells.resize_with(rows, Vec::new);
        for row in &mut self.cells {
            row.resize_with(cols, || Cell {
                text: " ".into(),
                hl: 0,
            });
        }
    }

    fn line(&mut self, row: usize, col: usize, cells: &Value) {
        let Some(cells) = cells.as_array() else {
            return;
        };
        let Some(line) = self.cells.get_mut(row) else {
            return;
        };
        let mut col = col;
        let mut hl = 0;
        for cell in cells {
            let Some(c) = cell.as_array() else { continue };
            let t = c.first().map(text).unwrap_or_default();
            if let Some(h) = c.get(1).and_then(Value::as_u64) {
                hl = h;
            }
            let repeat = c.get(2).and_then(Value::as_u64).unwrap_or(1) as usize;
            for _ in 0..repeat {
                if let Some(slot) = line.get_mut(col) {
                    *slot = Cell {
                        text: t.clone(),
                        hl,
                    };
                }
                col += 1;
            }
        }
    }

    fn scroll(&mut self, top: usize, bot: usize, left: usize, right: usize, rows: i64) {
        let height = bot.saturating_sub(top);
        if rows == 0 || height == 0 {
            return;
        }
        let bot = bot.min(self.cells.len());
        if top >= bot {
            return;
        }
        // Only the scrolled region is copied, before it is written over.
        let before: Vec<Vec<Cell>> = self.cells[top..bot].to_vec();
        for r in top..bot {
            let src = r as i64 + rows;
            if src < top as i64 || src >= bot as i64 {
                continue;
            }
            let from = &before[src as usize - top];
            let end = right.min(self.cells[r].len()).min(from.len());
            if left < end {
                self.cells[r][left..end].clone_from_slice(&from[left..end]);
            }
        }
    }

    /// The grid as a screen. The default background is left to the window.
    pub fn screen(&self) -> Screen {
        let rows = self.cells.len();
        let cols = self.cells.first().map_or(0, Vec::len);
        let lines = self
            .cells
            .iter()
            .map(|row| {
                let mut out = Line::default();
                let mut i = 0;
                while i < row.len() {
                    let cell = &row[i];
                    // Neovim sends a double-width character followed by an
                    // empty cell for its second column.
                    let wide = row.get(i + 1).is_some_and(|n| n.text.is_empty());
                    let width = if wide { 2 } else { 1 };
                    let apart = wide || cell.text.chars().next().is_some_and(set_apart);
                    let style = self.style(cell.hl);
                    match out.runs.last_mut() {
                        Some(last) if last.style == style && !last.apart && !apart => {
                            last.text.push_str(&cell.text);
                            last.width += width;
                        }
                        _ => out.runs.push(Run {
                            text: cell.text.clone(),
                            width,
                            wide,
                            apart,
                            style,
                        }),
                    }
                    i += width;
                }
                out
            })
            .collect();
        let (row, col) = self.cursor;
        let cursor = (row < rows && self.cmdline.is_none()).then(|| Cursor {
            row,
            col,
            shape: self
                .modes
                .get(self.mode)
                .copied()
                .unwrap_or(CursorShape::Block),
        });
        Screen {
            cols,
            rows,
            lines,
            cursor,
            background: self.bg,
        }
    }

    fn style(&self, id: u64) -> Style {
        let hl = self.hl.get(&id).copied().unwrap_or_default();
        let mut fg = hl.fg.unwrap_or(self.fg);
        // The colour scheme's own background is the window's to draw, so the
        // prompt looks the same whichever scheme the user has.
        let mut bg = hl.bg.filter(|&b| b != self.bg);
        if hl.reverse {
            let back = bg.unwrap_or(self.bg);
            bg = Some(fg);
            fg = back;
        }
        Style {
            fg,
            bg,
            bold: hl.bold,
            italic: hl.italic,
            underline: hl.underline,
            strike: hl.strike,
        }
    }

    pub fn mode(&self) -> &str {
        &self.mode_name
    }

    pub fn popup(&self) -> Option<&Popup> {
        self.popup.as_ref()
    }

    pub fn cmdline(&self) -> Option<&Cmdline> {
        self.cmdline.as_ref()
    }

    pub fn message(&self) -> Option<&str> {
        self.message.as_deref()
    }

    /// Forget the last message. Neovim clears it only by showing another, so
    /// without this one would outlive the keys and the text it was about.
    pub fn forget_message(&mut self) {
        self.message = None;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn conv(j: &serde_json::Value) -> Value {
        match j {
            serde_json::Value::Null => Value::Nil,
            serde_json::Value::Bool(b) => Value::from(*b),
            serde_json::Value::Number(n) => Value::from(n.as_i64().unwrap()),
            serde_json::Value::String(s) => Value::from(s.as_str()),
            serde_json::Value::Array(a) => Value::Array(a.iter().map(conv).collect()),
            serde_json::Value::Object(o) => Value::Map(
                o.iter()
                    .map(|(k, v)| (Value::from(k.as_str()), conv(v)))
                    .collect(),
            ),
        }
    }

    // Redraw batches are written as JSON in the tests for readability.
    fn v(json: &str) -> Value {
        conv(&serde_json::from_str(json).unwrap())
    }

    fn ui(batches: &str) -> UiState {
        let mut u = UiState::default();
        let Value::Array(b) = v(batches) else {
            panic!()
        };
        u.apply(&b);
        u
    }

    #[test]
    fn a_resized_grid_is_blank() {
        let u = ui(r#"[["grid_resize",[1,4,2]]]"#);
        let s = u.screen();
        assert_eq!((s.cols, s.rows), (4, 2));
        assert_eq!(s.text(), vec!["", ""]);
    }

    #[test]
    fn grid_line_writes_cells_and_repeats() {
        let u = ui(r#"[["grid_resize",[1,6,1]],["grid_line",[1,0,0,[["a"],["b",0,3]],false]]]"#);
        assert_eq!(u.screen().text(), vec!["abbb"]);
    }

    #[test]
    fn a_highlight_id_carries_over_to_cells_that_omit_it() {
        let u = ui(r#"[
            ["hl_attr_define",[7,{"foreground":16711680,"bold":true},{},[]]],
            ["grid_resize",[1,3,1]],
            ["grid_line",[1,0,0,[["x",7],["y"]],false]]
        ]"#);
        let s = u.screen();
        let run = &s.lines[0].runs[0];
        assert_eq!(run.text, "xy");
        assert_eq!(run.style.fg, Rgb::new(255, 0, 0));
        assert!(run.style.bold);
    }

    #[test]
    fn default_colours_apply_to_unhighlighted_text() {
        let u = ui(r#"[
            ["default_colors_set",[255,0,0,0,0]],
            ["grid_resize",[1,1,1]],
            ["grid_line",[1,0,0,[["x"]],false]]
        ]"#);
        let s = u.screen();
        assert_eq!(s.lines[0].runs[0].style.fg, Rgb::new(0, 0, 255));
        assert_eq!(s.background, Rgb::new(0, 0, 0));
        assert_eq!(s.lines[0].runs[0].style.bg, None);
    }

    #[test]
    fn a_highlight_painting_the_default_background_leaves_it_to_the_window() {
        let u = ui(r#"[
            ["default_colors_set",[16777215,1052688,0,0,0]],
            ["hl_attr_define",[7,{"background":1052688},{},[]]],
            ["hl_attr_define",[8,{"background":3355443},{},[]]],
            ["grid_resize",[1,2,1]],
            ["grid_line",[1,0,0,[["x",7],["y",8]],false]]
        ]"#);
        let runs = &u.screen().lines[0].runs;
        assert_eq!(runs[0].style.bg, None);
        assert_eq!(runs[1].style.bg, Some(Rgb::new(0x33, 0x33, 0x33)));
    }

    #[test]
    fn a_wide_character_continuation_adds_no_text() {
        let u = ui(r#"[["grid_resize",[1,3,1]],["grid_line",[1,0,0,[["日"],[""],["x"]],false]]]"#);
        assert_eq!(u.screen().text(), vec!["日x"]);
    }

    #[test]
    fn a_wide_character_is_a_run_of_its_own_two_columns_wide() {
        let u = ui(r#"[["grid_resize",[1,3,1]],["grid_line",[1,0,0,[["日"],[""],["x"]],false]]]"#);
        let runs = &u.screen().lines[0].runs;
        assert_eq!(
            (runs[0].text.as_str(), runs[0].width, runs[0].wide),
            ("日", 2, true)
        );
        assert_eq!((runs[1].text.as_str(), runs[1].wide), ("x", false));
    }

    #[test]
    fn half_width_katakana_in_the_prompt_is_drawn_a_cell_at_a_time() {
        let u = ui(
            r#"[["grid_resize",[1,4,1]],["grid_line",[1,0,0,[["a"],["ｱ"],["ｲ"],["b"]],false]]]"#,
        );
        let screen = u.screen();
        let runs: Vec<(&str, bool)> = screen.lines[0]
            .runs
            .iter()
            .map(|r| (r.text.as_str(), r.apart))
            .collect();
        assert_eq!(runs, [("a", false), ("ｱ", true), ("ｲ", true), ("b", false)]);
    }

    #[test]
    fn wide_characters_side_by_side_keep_a_run_each() {
        let u = ui(
            r#"[["grid_resize",[1,4,1]],["grid_line",[1,0,0,[["日"],[""],["本"],[""]],false]]]"#,
        );
        let runs = &u.screen().lines[0].runs;
        assert_eq!(runs.len(), 2);
        assert_eq!((runs[1].text.as_str(), runs[1].width), ("本", 2));
    }

    #[test]
    fn grid_scroll_moves_rows_up() {
        let u = ui(r#"[
            ["grid_resize",[1,1,3]],
            ["grid_line",[1,0,0,[["a"]],false],[1,1,0,[["b"]],false],[1,2,0,[["c"]],false]],
            ["grid_scroll",[1,0,3,0,1,1,0]]
        ]"#);
        assert_eq!(u.screen().text()[..2], ["b".to_string(), "c".to_string()]);
    }

    #[test]
    fn the_cursor_shape_follows_the_mode() {
        let u = ui(r#"[
            ["grid_resize",[1,4,1]],
            ["mode_info_set",[true,[{"cursor_shape":"block","name":"normal"},{"cursor_shape":"vertical","name":"insert"}]]],
            ["mode_change",["insert",1]],
            ["grid_cursor_goto",[1,0,2]]
        ]"#);
        let c = u.screen().cursor.unwrap();
        assert_eq!((c.row, c.col, c.shape), (0, 2, CursorShape::Beam));
        assert_eq!(u.mode(), "insert");
    }

    #[test]
    fn the_completion_menu_is_kept_outside_the_grid() {
        let u = ui(r#"[
            ["popupmenu_show",[[["@login-ui","s","web","Building the login"],["@tests","s","",""]],0,1,4,1]],
            ["popupmenu_select",[1]]
        ]"#);
        let p = u.popup().unwrap();
        assert_eq!(p.items[0].word, "@login-ui");
        assert_eq!(p.items[0].info, "Building the login");
        assert_eq!(p.selected, Some(1));
        assert_eq!((p.row, p.col), (1, 4));
        let u2 = {
            let mut u = u.clone();
            u.apply(&[v(r#"["popupmenu_hide",[]]"#)]);
            u
        };
        assert!(u2.popup().is_none());
    }

    #[test]
    fn the_command_line_is_reported_and_hides_the_grid_cursor() {
        let u = ui(r#"[
            ["grid_resize",[1,4,1]],
            ["cmdline_show",[[[0,"wq"]],2,":","",0,1]]
        ]"#);
        let c = u.cmdline().unwrap();
        assert_eq!((c.firstc.as_str(), c.text.as_str(), c.pos), (":", "wq", 2));
        assert!(u.screen().cursor.is_none());
    }

    #[test]
    fn the_last_message_is_kept_until_cleared() {
        let u = ui(r#"[["msg_show",["echo",[[0,"hello"]],false]]]"#);
        assert_eq!(u.message(), Some("hello"));
    }

    #[test]
    fn a_message_is_forgotten_once_the_prompt_is_touched_again() {
        let mut u = ui(r#"[["msg_show",["echo",[[0,"--No lines in buffer--"]],false]]]"#);
        u.forget_message();
        assert_eq!(u.message(), None);
    }
}
