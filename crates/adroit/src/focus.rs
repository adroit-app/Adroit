//! Which area of the window has the keyboard, and where a window motion moves it.

/// The areas of the window, laid out as the sidebar on the left, the session
/// console above the prompt in the middle, and the details on the right.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Area {
    Sidebar,
    Console,
    Prompt,
    Details,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Dir {
    Left,
    Down,
    Up,
    Right,
}

impl Dir {
    /// The direction a key names after `Ctrl-w`: `h j k l` or an arrow.
    pub fn from_key(key: &str) -> Option<Self> {
        match key {
            "h" | "ArrowLeft" => Some(Self::Left),
            "j" | "ArrowDown" => Some(Self::Down),
            "k" | "ArrowUp" => Some(Self::Up),
            "l" | "ArrowRight" => Some(Self::Right),
            _ => None,
        }
    }
}

/// The area a window motion from `from` lands in. A motion with nothing in
/// that direction stays where it is.
pub fn moved(from: Area, dir: Dir) -> Area {
    use Area::*;
    use Dir::*;
    match (from, dir) {
        (Sidebar, Right) => Console,
        (Console, Left) | (Prompt, Left) => Sidebar,
        (Console, Right) | (Prompt, Right) => Details,
        (Console, Down) => Prompt,
        (Prompt, Up) => Console,
        (Details, Left) => Console,
        (area, _) => area,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use Area::*;
    use Dir::*;

    #[test]
    fn the_console_sits_above_the_prompt() {
        assert_eq!(moved(Console, Down), Prompt);
        assert_eq!(moved(Prompt, Up), Console);
    }

    #[test]
    fn the_sidebar_is_left_of_both_and_the_details_right_of_both() {
        for middle in [Console, Prompt] {
            assert_eq!(moved(middle, Left), Sidebar);
            assert_eq!(moved(middle, Right), Details);
        }
        assert_eq!(moved(Sidebar, Right), Console);
        assert_eq!(moved(Details, Left), Console);
    }

    #[test]
    fn a_motion_toward_the_edge_stays_put() {
        assert_eq!(moved(Sidebar, Left), Sidebar);
        assert_eq!(moved(Details, Right), Details);
        assert_eq!(moved(Console, Up), Console);
        assert_eq!(moved(Prompt, Down), Prompt);
        assert_eq!(moved(Sidebar, Down), Sidebar);
    }

    #[test]
    fn hjkl_and_the_arrows_name_directions() {
        assert_eq!(Dir::from_key("h"), Some(Left));
        assert_eq!(Dir::from_key("ArrowDown"), Some(Down));
        assert_eq!(Dir::from_key("x"), None);
    }
}
