//! Which area of the window has the keyboard, and where a window motion moves it.

use crate::content::{Content, WindowId};

/// The areas of the window, laid out as the sidebar on the left, the content's
/// windows in the middle, and the details on the right. A window shows its
/// program as the console, above the prompt while it shows a session.
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

/// Where the keys are: an area, and the window it lies in when it is the
/// console or the prompt of one. With nothing open, the middle has no window.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Spot {
    pub area: Area,
    pub window: Option<WindowId>,
}

impl Spot {
    pub const SIDEBAR: Spot = Spot {
        area: Area::Sidebar,
        window: None,
    };
    pub const DETAILS: Spot = Spot {
        area: Area::Details,
        window: None,
    };

    pub fn console(window: Option<WindowId>) -> Self {
        Spot {
            area: Area::Console,
            window,
        }
    }

    pub fn prompt(window: Option<WindowId>) -> Self {
        Spot {
            area: Area::Prompt,
            window,
        }
    }
}

/// Which of the side panels are shown.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Shown {
    pub sidebar: bool,
    pub details: bool,
}

/// The spot a window motion from `from` lands in. The sidebar lies left of
/// every window and the details right of them; a session's prompt lies below
/// its console. `prompt` tells whether a window showing a thing, or the middle
/// with nothing open, has a prompt. A motion with nothing in that direction,
/// or toward a panel hidden, stays where it is.
pub fn moved<T: Clone + PartialEq>(
    from: Spot,
    dir: Dir,
    content: &Content<T>,
    prompt: impl Fn(Option<&T>) -> bool,
    shown: Shown,
) -> Spot {
    let entered = |d: Dir| Spot::console(content.entered_from(d));
    let here = prompt(content.focused());
    match (from.area, dir) {
        (Area::Sidebar, Dir::Right) | (Area::Details, Dir::Left) => entered(dir),
        (Area::Sidebar | Area::Details, _) => from,
        (Area::Console, Dir::Down) if here => Spot::prompt(from.window),
        (Area::Prompt, Dir::Up) => Spot::console(from.window),
        (_, _) => match content.neighbour(dir) {
            Some(w) if dir == Dir::Up && prompt(content.shows(w)) => Spot::prompt(Some(w)),
            Some(w) => Spot::console(Some(w)),
            None if dir == Dir::Left && shown.sidebar => Spot::SIDEBAR,
            None if dir == Dir::Right && shown.details => Spot::DETAILS,
            None => from,
        },
    }
}

/// Every spot shown, in the order the next window visits them: the sidebar,
/// each window's console and prompt, the details.
fn order<T: Clone + PartialEq>(
    content: &Content<T>,
    prompt: impl Fn(Option<&T>) -> bool,
    shown: Shown,
) -> Vec<Spot> {
    let mut out = Vec::new();
    if shown.sidebar {
        out.push(Spot::SIDEBAR);
    }
    match content.current() {
        Some(tab) => {
            for w in tab.windows() {
                out.push(Spot::console(Some(w.id)));
                if prompt(Some(&w.shows)) {
                    out.push(Spot::prompt(Some(w.id)));
                }
            }
        }
        None => {
            out.push(Spot::console(None));
            if prompt(None) {
                out.push(Spot::prompt(None));
            }
        }
    }
    if shown.details {
        out.push(Spot::DETAILS);
    }
    out
}

/// The spot after `from`, or before it unless `forward`, among those shown,
/// going round at the ends.
pub fn cycled<T: Clone + PartialEq>(
    from: Spot,
    forward: bool,
    content: &Content<T>,
    prompt: impl Fn(Option<&T>) -> bool,
    shown: Shown,
) -> Spot {
    let order = order(content, prompt, shown);
    let at = order.iter().position(|s| *s == from).unwrap_or(0);
    let n = order.len();
    order[if forward {
        (at + 1) % n
    } else {
        (at + n - 1) % n
    }]
}

/// The first spot shown, or the last unless `first`.
pub fn corner<T: Clone + PartialEq>(
    first: bool,
    content: &Content<T>,
    prompt: impl Fn(Option<&T>) -> bool,
    shown: Shown,
) -> Spot {
    let order = order(content, prompt, shown);
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
    use crate::content::Axis;
    use Dir::*;

    const ALL_SHOWN: Shown = Shown {
        sidebar: true,
        details: true,
    };

    /// Whether a thing has a prompt: a session, written `s…`, or nothing open.
    fn prompt(t: Option<&&str>) -> bool {
        t.is_none_or(|t| t.starts_with('s'))
    }

    /// A tab of a session beside a shell: `(s1 | term)`, the shell focused.
    fn session_and_shell() -> (Content<&'static str>, WindowId, WindowId) {
        let mut c = Content::default();
        c.open("s1");
        c.split(Axis::Row, "term");
        let w = c.current().unwrap().windows();
        let (a, b) = (w[0].id, w[1].id);
        (c, a, b)
    }

    #[test]
    fn a_session_s_console_sits_above_its_prompt() {
        let (mut c, s, _) = session_and_shell();
        c.focus(s);
        assert_eq!(
            moved(Spot::console(Some(s)), Down, &c, prompt, ALL_SHOWN),
            Spot::prompt(Some(s))
        );
        assert_eq!(
            moved(Spot::prompt(Some(s)), Up, &c, prompt, ALL_SHOWN),
            Spot::console(Some(s))
        );
    }

    #[test]
    fn a_motion_moves_to_the_window_beside_the_focused_one() {
        let (mut c, s, sh) = session_and_shell();
        assert_eq!(
            moved(Spot::console(Some(sh)), Left, &c, prompt, ALL_SHOWN),
            Spot::console(Some(s))
        );
        c.focus(s);
        assert_eq!(
            moved(Spot::prompt(Some(s)), Right, &c, prompt, ALL_SHOWN),
            Spot::console(Some(sh))
        );
    }

    #[test]
    fn moving_up_into_a_session_s_window_lands_on_its_prompt() {
        let mut c = Content::default();
        c.open("s1");
        c.split(Axis::Column, "term");
        let top = c.current().unwrap().windows()[0].id;
        let bottom = c.focused_window();
        assert_eq!(
            moved(Spot::console(bottom), Up, &c, prompt, ALL_SHOWN),
            Spot::prompt(Some(top))
        );
    }

    #[test]
    fn the_sidebar_lies_left_of_the_windows_and_the_details_right_of_them() {
        let (mut c, s, sh) = session_and_shell();
        c.focus(s);
        assert_eq!(
            moved(Spot::console(Some(s)), Left, &c, prompt, ALL_SHOWN),
            Spot::SIDEBAR
        );
        assert_eq!(
            moved(Spot::SIDEBAR, Right, &c, prompt, ALL_SHOWN),
            Spot::console(Some(s))
        );
        c.focus(sh);
        assert_eq!(
            moved(Spot::console(Some(sh)), Right, &c, prompt, ALL_SHOWN),
            Spot::DETAILS
        );
        assert_eq!(
            moved(Spot::DETAILS, Left, &c, prompt, ALL_SHOWN),
            Spot::console(Some(sh))
        );
    }

    #[test]
    fn coming_in_from_a_panel_lands_on_a_window_along_its_side() {
        let (c, s, sh) = session_and_shell();
        assert_eq!(
            moved(Spot::SIDEBAR, Right, &c, prompt, ALL_SHOWN),
            Spot::console(Some(s))
        );
        assert_eq!(
            moved(Spot::DETAILS, Left, &c, prompt, ALL_SHOWN),
            Spot::console(Some(sh))
        );
    }

    #[test]
    fn a_motion_toward_the_edge_or_a_hidden_panel_stays_put() {
        let (mut c, s, sh) = session_and_shell();
        assert_eq!(
            moved(Spot::console(Some(sh)), Up, &c, prompt, ALL_SHOWN),
            Spot::console(Some(sh))
        );
        assert_eq!(
            moved(Spot::console(Some(sh)), Down, &c, prompt, ALL_SHOWN),
            Spot::console(Some(sh))
        );
        assert_eq!(
            moved(Spot::SIDEBAR, Left, &c, prompt, ALL_SHOWN),
            Spot::SIDEBAR
        );
        assert_eq!(
            moved(Spot::SIDEBAR, Down, &c, prompt, ALL_SHOWN),
            Spot::SIDEBAR
        );
        assert_eq!(
            moved(Spot::DETAILS, Right, &c, prompt, ALL_SHOWN),
            Spot::DETAILS
        );
        let hidden = Shown {
            sidebar: false,
            details: false,
        };
        assert_eq!(
            moved(Spot::console(Some(sh)), Right, &c, prompt, hidden),
            Spot::console(Some(sh))
        );
        c.focus(s);
        assert_eq!(
            moved(Spot::console(Some(s)), Left, &c, prompt, hidden),
            Spot::console(Some(s))
        );
    }

    #[test]
    fn with_nothing_open_the_middle_is_a_console_above_a_prompt() {
        let c = Content::<&str>::default();
        let middle = Spot::console(None);
        assert_eq!(moved(Spot::SIDEBAR, Right, &c, prompt, ALL_SHOWN), middle);
        assert_eq!(
            moved(middle, Down, &c, prompt, ALL_SHOWN),
            Spot::prompt(None)
        );
        assert_eq!(moved(middle, Left, &c, prompt, ALL_SHOWN), Spot::SIDEBAR);
        assert_eq!(
            moved(Spot::prompt(None), Right, &c, prompt, ALL_SHOWN),
            Spot::DETAILS
        );
    }

    #[test]
    fn the_next_window_goes_from_the_sidebar_through_every_window_to_the_details_and_round() {
        let (c, s, sh) = session_and_shell();
        let order = [
            Spot::SIDEBAR,
            Spot::console(Some(s)),
            Spot::prompt(Some(s)),
            Spot::console(Some(sh)),
            Spot::DETAILS,
            Spot::SIDEBAR,
        ];
        for pair in order.windows(2) {
            assert_eq!(
                cycled(pair[0], true, &c, prompt, ALL_SHOWN),
                pair[1],
                "{pair:?}"
            );
            assert_eq!(
                cycled(pair[1], false, &c, prompt, ALL_SHOWN),
                pair[0],
                "{pair:?}"
            );
        }
    }

    #[test]
    fn the_next_window_passes_over_a_hidden_panel() {
        let (c, s, sh) = session_and_shell();
        let shown = Shown {
            sidebar: false,
            details: true,
        };
        assert_eq!(
            cycled(Spot::DETAILS, true, &c, prompt, shown),
            Spot::console(Some(s))
        );
        assert_eq!(
            cycled(Spot::console(Some(sh)), true, &c, prompt, shown),
            Spot::DETAILS
        );
    }

    #[test]
    fn the_first_spot_is_the_sidebar_and_the_last_the_details_or_the_window_nearest() {
        let (c, s, sh) = session_and_shell();
        assert_eq!(corner(true, &c, prompt, ALL_SHOWN), Spot::SIDEBAR);
        assert_eq!(corner(false, &c, prompt, ALL_SHOWN), Spot::DETAILS);
        let middle = Shown {
            sidebar: false,
            details: false,
        };
        assert_eq!(corner(true, &c, prompt, middle), Spot::console(Some(s)));
        assert_eq!(corner(false, &c, prompt, middle), Spot::console(Some(sh)));
    }

    #[test]
    fn the_keys_in_a_prompt_that_goes_away_go_to_the_console_above_it() {
        // (area, the prompt shown) -> where the keys are
        let table = [
            (Area::Prompt, false, Area::Console),
            (Area::Prompt, true, Area::Prompt),
            (Area::Console, false, Area::Console),
            (Area::Sidebar, false, Area::Sidebar),
            (Area::Details, false, Area::Details),
        ];
        for (area, prompt, want) in table {
            assert_eq!(settled(area, prompt), want, "{area:?} {prompt}");
        }
    }

    #[test]
    fn hjkl_and_the_arrows_name_directions() {
        assert_eq!(Dir::from_key("h"), Some(Left));
        assert_eq!(Dir::from_key("ArrowDown"), Some(Down));
        assert_eq!(Dir::from_key("x"), None);
    }
}
