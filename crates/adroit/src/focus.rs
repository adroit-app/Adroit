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
    /// The direction a key names after `Ctrl-q`: `h j k l` or an arrow.
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

/// The area a window motion from `from` lands in, `prompt` telling whether
/// the prompt is shown below the console. A motion with nothing in that
/// direction stays where it is.
pub fn moved(from: Area, dir: Dir, prompt: bool) -> Area {
    use Area::*;
    use Dir::*;
    match (settled(from, prompt), dir) {
        (Sidebar, Right) => Console,
        (Console, Left) | (Prompt, Left) => Sidebar,
        (Console, Right) | (Prompt, Right) => Details,
        (Console, Down) if prompt => Prompt,
        (Prompt, Up) => Console,
        (Details, Left) => Console,
        (area, _) => area,
    }
}

/// Which of the areas that can be hidden are shown; the console always is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Shown {
    pub sidebar: bool,
    pub prompt: bool,
    pub details: bool,
}

impl Shown {
    /// The areas shown, in the order the next area visits them.
    fn order(self) -> Vec<Area> {
        [
            (Area::Sidebar, self.sidebar),
            (Area::Console, true),
            (Area::Prompt, self.prompt),
            (Area::Details, self.details),
        ]
        .into_iter()
        .filter_map(|(area, on)| on.then_some(area))
        .collect()
    }
}

/// The area after `from`, or before it unless `forward`, among those shown,
/// going round at the ends.
pub fn cycled(from: Area, forward: bool, shown: Shown) -> Area {
    let order = shown.order();
    let at = order
        .iter()
        .position(|a| *a == settled(from, shown.prompt))
        .unwrap_or(0);
    let n = order.len();
    order[if forward {
        (at + 1) % n
    } else {
        (at + n - 1) % n
    }]
}

/// The first area shown, or the last unless `first`.
pub fn corner(first: bool, shown: Shown) -> Area {
    let order = shown.order();
    if first {
        order[0]
    } else {
        order[order.len() - 1]
    }
}

/// Where the keys are, `prompt` telling whether the prompt is shown: never in
/// a prompt that is not, which leaves them to the console above it.
pub fn settled(area: Area, prompt: bool) -> Area {
    match area {
        Area::Prompt if !prompt => Area::Console,
        area => area,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use Area::*;
    use Dir::*;

    #[test]
    fn the_console_sits_above_the_prompt() {
        assert_eq!(moved(Console, Down, true), Prompt);
        assert_eq!(moved(Prompt, Up, true), Console);
    }

    #[test]
    fn the_sidebar_is_left_of_both_and_the_details_right_of_both() {
        for middle in [Console, Prompt] {
            assert_eq!(moved(middle, Left, true), Sidebar);
            assert_eq!(moved(middle, Right, true), Details);
        }
        assert_eq!(moved(Sidebar, Right, true), Console);
        assert_eq!(moved(Details, Left, true), Console);
    }

    #[test]
    fn a_motion_toward_the_edge_stays_put() {
        assert_eq!(moved(Sidebar, Left, true), Sidebar);
        assert_eq!(moved(Details, Right, true), Details);
        assert_eq!(moved(Console, Up, true), Console);
        assert_eq!(moved(Prompt, Down, true), Prompt);
        assert_eq!(moved(Sidebar, Down, true), Sidebar);
    }

    #[test]
    fn with_no_prompt_shown_the_console_has_nothing_below_it() {
        assert_eq!(moved(Console, Down, false), Console);
        assert_eq!(moved(Console, Up, false), Console);
    }

    #[test]
    fn with_no_prompt_shown_the_sides_still_lead_to_the_console() {
        assert_eq!(moved(Sidebar, Right, false), Console);
        assert_eq!(moved(Details, Left, false), Console);
        assert_eq!(moved(Console, Left, false), Sidebar);
        assert_eq!(moved(Console, Right, false), Details);
    }

    #[test]
    fn no_motion_lands_in_a_prompt_that_is_not_shown() {
        for from in [Sidebar, Console, Prompt, Details] {
            for dir in [Left, Down, Up, Right] {
                assert_ne!(moved(from, dir, false), Prompt, "{from:?} {dir:?}");
            }
        }
    }

    #[test]
    fn the_keys_in_a_prompt_that_goes_away_go_to_the_console_above_it() {
        // (area, the prompt shown) -> where the keys are
        let table = [
            (Prompt, false, Console),
            (Prompt, true, Prompt),
            (Console, false, Console),
            (Sidebar, false, Sidebar),
            (Details, false, Details),
        ];
        for (area, prompt, want) in table {
            assert_eq!(settled(area, prompt), want, "{area:?} {prompt}");
        }
    }

    const ALL_SHOWN: Shown = Shown {
        sidebar: true,
        prompt: true,
        details: true,
    };

    #[test]
    fn the_next_area_goes_from_the_sidebar_down_the_middle_to_the_details_and_round() {
        let order = [Sidebar, Console, Prompt, Details, Sidebar];
        for pair in order.windows(2) {
            assert_eq!(cycled(pair[0], true, ALL_SHOWN), pair[1], "{pair:?}");
            assert_eq!(cycled(pair[1], false, ALL_SHOWN), pair[0], "{pair:?}");
        }
    }

    #[test]
    fn the_next_area_passes_over_what_is_hidden() {
        let shown = Shown {
            sidebar: false,
            prompt: false,
            details: true,
        };
        assert_eq!(cycled(Console, true, shown), Details);
        assert_eq!(cycled(Details, true, shown), Console);
        assert_eq!(cycled(Console, false, shown), Details);
    }

    #[test]
    fn the_first_area_is_the_sidebar_and_the_last_the_details_or_what_is_shown_nearest() {
        assert_eq!(corner(true, ALL_SHOWN), Sidebar);
        assert_eq!(corner(false, ALL_SHOWN), Details);
        let middle = Shown {
            sidebar: false,
            prompt: true,
            details: false,
        };
        assert_eq!(corner(true, middle), Console);
        assert_eq!(corner(false, middle), Prompt);
        let console_only = Shown {
            prompt: false,
            ..middle
        };
        assert_eq!(corner(false, console_only), Console);
    }

    #[test]
    fn hjkl_and_the_arrows_name_directions() {
        assert_eq!(Dir::from_key("h"), Some(Left));
        assert_eq!(Dir::from_key("ArrowDown"), Some(Down));
        assert_eq!(Dir::from_key("x"), None);
    }
}
