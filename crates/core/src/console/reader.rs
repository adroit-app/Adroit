//! The keys the console's reader takes from the window: those that leave it,
//! and those that move past what it holds into the program's own scrolling.

use adroit_model::Key;

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
pub fn past_edge(key: &Key, at: Edge) -> Option<i32> {
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

/// Where a key leaves the reader for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Exit {
    Live,
    Prompt,
}

/// The normal-mode keys that leave the reader. Other modes keep their keys
/// so searches and selections can be finished inside Neovim.
pub fn exit(key: &Key, normal: bool) -> Option<Exit> {
    if !normal || key.ctrl || key.alt || key.meta {
        return None;
    }
    match key.key.as_str() {
        "Escape" | "q" | "G" => Some(Exit::Live),
        "i" | "a" | "I" | "A" | "o" | "O" => Some(Exit::Prompt),
        _ => None,
    }
}

/// Claude Code's key for scrolling to its newest message.
pub fn newest() -> Key {
    Key::new("End").ctrl()
}

/// The key that scrolls the session to its start when `key` after
/// `previous` moves the reader to its first line, as `gg` does. `None`
/// for any other key. The wheel would stop short
/// of a long session, as a terminal turns it only so far at once.
pub fn to_start(key: &Key, previous: Option<&Key>) -> Option<Key> {
    let plain = |k: &Key| !k.ctrl && !k.alt && !k.meta;
    if !plain(key) {
        return None;
    }
    match key.key.as_str() {
        "g" if previous.is_some_and(|p| plain(p) && p.key == "g") => Some(Key::new("Home").ctrl()),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key(name: &str) -> Key {
        Key::new(name)
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

    #[test]
    fn leaving_read_mode_uses_the_same_keys_for_live_and_prompt() {
        for k in ["q", "Escape", "G"] {
            assert_eq!(exit(&key(k), true), Some(Exit::Live), "{k}");
            assert_eq!(exit(&key(k), false), None, "{k}");
            assert_eq!(exit(&key(k).ctrl(), true), None, "{k}");
        }
        for k in ["i", "a", "I", "A", "o", "O"] {
            assert_eq!(exit(&key(k), true), Some(Exit::Prompt), "{k}");
            assert_eq!(exit(&key(k), false), None, "{k}");
            assert_eq!(exit(&key(k).meta(), true), None, "{k}");
        }
        assert_eq!(exit(&key("G").shift(), true), Some(Exit::Live));
        assert_eq!(exit(&key("j"), true), None);
    }

    #[test]
    fn going_to_the_first_line_scrolls_the_session_all_the_way() {
        let g = key("g");
        assert_eq!(to_start(&key("G").shift(), None), None);
        assert_eq!(to_start(&g, Some(&g)), Some(key("Home").ctrl()));
        // A lone g waits for the next key; one after another key is a new start.
        assert_eq!(to_start(&g, None), None);
        assert_eq!(to_start(&g, Some(&key("j"))), None);
        assert_eq!(to_start(&key("j"), Some(&g)), None);
        assert_eq!(to_start(&key("g").ctrl(), Some(&g)), None);
    }
}
