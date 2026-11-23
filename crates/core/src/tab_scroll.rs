//! How the one line of tabs scrolls sideways where it is wider than the
//! room it has.

/// How far off a measured width may be and still count as fitting: widths
/// are fractions of a pixel, and a scroll stops at whole ones.
const SLACK: f64 = 1.0;

/// How much of what is in view one press of a button scrolls past; the rest
/// stays in view, so the eye keeps its place.
const PRESS: f64 = 0.8;

/// The line of tabs as measured, in the page's pixels.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct TabScroll {
    /// The room for the line and its buttons together. Whether the buttons
    /// show is told from this and not from `view`, which they narrow: told
    /// from that, showing them would be a reason to keep showing them.
    pub room: f64,
    /// How much of the line is in view.
    pub view: f64,
    /// How wide the tabs are together.
    pub length: f64,
    /// How far the line is scrolled.
    pub left: f64,
}

impl TabScroll {
    /// Whether the tabs do not fit, so that the buttons show.
    pub fn overflows(&self) -> bool {
        self.room > 0.0 && self.length > self.room + SLACK
    }

    /// Whether tabs are out of view to the left.
    pub fn more_left(&self) -> bool {
        self.overflows() && self.left > SLACK
    }

    /// Whether tabs are out of view to the right.
    pub fn more_right(&self) -> bool {
        self.overflows() && self.left + self.view < self.length - SLACK
    }

    /// How far a press of a button scrolls.
    pub fn press(&self) -> f64 {
        (self.view * PRESS).round()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scroll(length: f64, left: f64) -> TabScroll {
        TabScroll {
            room: 300.0,
            view: 250.0,
            length,
            left,
        }
    }

    #[test]
    fn tabs_that_fit_the_room_show_no_buttons() {
        let s = TabScroll {
            view: 300.0,
            ..scroll(300.4, 0.0)
        };
        assert!(!s.overflows() && !s.more_left() && !s.more_right());
    }

    #[test]
    fn tabs_wider_than_the_room_show_the_buttons() {
        assert!(scroll(310.0, 0.0).overflows());
    }

    #[test]
    fn the_buttons_go_once_the_tabs_fit_the_room_without_them() {
        assert!(!scroll(280.0, 0.0).overflows());
    }

    #[test]
    fn a_line_not_measured_yet_shows_no_buttons() {
        assert!(
            !TabScroll {
                length: 500.0,
                ..TabScroll::default()
            }
            .overflows()
        );
    }

    #[test]
    fn at_the_left_end_only_the_right_has_more() {
        let s = scroll(600.0, 0.0);
        assert!(!s.more_left() && s.more_right());
    }

    #[test]
    fn between_the_ends_both_sides_have_more() {
        let s = scroll(600.0, 100.0);
        assert!(s.more_left() && s.more_right());
    }

    #[test]
    fn at_the_right_end_only_the_left_has_more_whatever_fraction_is_left() {
        let s = scroll(600.6, 350.0);
        assert!(s.more_left() && !s.more_right());
    }

    #[test]
    fn a_press_leaves_some_of_what_was_in_view_in_view() {
        let by = scroll(600.0, 0.0).press();
        assert!(by > 0.0 && by < 250.0);
    }
}
