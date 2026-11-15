//! What every picker's list holds, whichever picker draws it: what is typed
//! to narrow it, and the row the cursor is on.

use crate::cursor;

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ListState {
    pub query: String,
    pub cursor: usize,
}

impl ListState {
    /// Typing narrows the list anew, so the cursor goes back to the best row.
    /// Narrow by `query` from now on, from the first row when it changed.
    pub fn set_query(&mut self, query: impl AsRef<str>) {
        let query = query.as_ref();
        if self.query != query {
            self.query = query.to_string();
            self.cursor = 0;
        }
    }

    /// The cursor moved by `delta` over `len` rows, going round either end.
    pub fn step(&mut self, delta: isize, len: usize) {
        self.cursor = cursor::wrap(self.cursor, delta, len);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn typing_narrows_anew_and_puts_the_cursor_back_on_the_first_row() {
        let mut list = ListState {
            cursor: 3,
            ..ListState::default()
        };
        list.set_query("adr");
        assert_eq!(list.query, "adr");
        assert_eq!(list.cursor, 0);
    }

    #[test]
    fn the_same_query_again_keeps_the_cursor_where_it_is() {
        let mut list = ListState::default();
        list.set_query("aあ");
        list.cursor = 2;
        list.set_query("aあ");
        assert_eq!(list.cursor, 2);
    }

    #[test]
    fn the_cursor_wraps_at_either_end() {
        let stepped = |cursor, delta, len| {
            let mut list = ListState {
                cursor,
                ..ListState::default()
            };
            list.step(delta, len);
            list.cursor
        };
        assert_eq!(stepped(0, 1, 3), 1);
        assert_eq!(stepped(2, 1, 3), 0);
        assert_eq!(stepped(0, -1, 3), 2);
        assert_eq!(stepped(0, 1, 0), 0);
    }
}
