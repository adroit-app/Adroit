//! A window dragged with the mouse, by its strip or by its tab: where it
//! would go were it let go now, and moving it there once it is.

use super::{Content, Dir, WindowId};

/// How far the pointer moves, in pixels, before a press becomes a drag, so
/// a click stays a click.
const THRESHOLD: f64 = 4.0;

/// The share of a window, from each of its sides, that drops onto that side;
/// the middle, past it on every side, drops nowhere.
const EDGE: f64 = 0.3;

/// Where a dragged window would go.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum DropAt {
    /// To that side of the window.
    Side(WindowId, Dir),
    /// Into the tab at that place among the tabs, beside its focused window.
    Tab(usize),
}

/// A window pressed to be dragged, and, once moved far enough to be dragged,
/// where it would go.
#[derive(Debug, Clone, PartialEq)]
pub struct WindowMove {
    pub window: WindowId,
    /// Where the pointer pressed it, in pixels.
    from: (f64, f64),
    /// Whether the pointer moved far enough from the press to drag it.
    pub dragging: bool,
    pub over: Option<DropAt>,
}

impl WindowMove {
    pub fn new(window: WindowId, from: (f64, f64)) -> Self {
        WindowMove {
            window,
            from,
            dragging: false,
            over: None,
        }
    }

    /// The pointer at `at`, over `over`.
    pub fn point(&mut self, at: (f64, f64), over: Option<DropAt>) {
        let (dx, dy) = (at.0 - self.from.0, at.1 - self.from.1);
        if !self.dragging && dx.hypot(dy) >= THRESHOLD {
            self.dragging = true;
        }
        if self.dragging {
            self.over = over;
        }
    }
}

/// The side of a window a point drops onto, the point given as shares of
/// the window's width and height from its top left: the side it is nearest,
/// when it is within [`EDGE`] of it.
pub fn side_at(x: f64, y: f64) -> Option<Dir> {
    [
        (x, Dir::Left),
        (1.0 - x, Dir::Right),
        (y, Dir::Up),
        (1.0 - y, Dir::Down),
    ]
    .into_iter()
    .filter(|(d, _)| *d < EDGE)
    .min_by(|a, b| a.0.total_cmp(&b.0))
    .map(|(_, dir)| dir)
}

impl<T: Clone + PartialEq> Content<T> {
    /// Move the window `id` where `to` says, as [`Content::place`] moves it;
    /// into a tab, beside its focused window, right of it. Nothing moves
    /// into the tab it is in.
    pub fn drop_window(&mut self, id: WindowId, to: DropAt) {
        match to {
            DropAt::Side(beside, side) => self.place(id, beside, side),
            DropAt::Tab(at) => {
                let Some(tab) = self.tabs().get(at) else {
                    return;
                };
                if tab.windows().iter().any(|w| w.id == id) {
                    return;
                }
                let beside = tab.focused().id;
                self.place(id, beside, Dir::Right);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::Axis;
    use super::*;

    type C = Content<&'static str>;

    fn id_of(c: &C, thing: &str) -> WindowId {
        c.tabs()
            .iter()
            .flat_map(|t| t.windows())
            .find(|w| w.shows == thing)
            .map(|w| w.id)
            .expect("a window shows it")
    }

    #[test]
    fn a_point_near_a_side_drops_onto_the_side_it_is_nearest() {
        assert_eq!(side_at(0.1, 0.5), Some(Dir::Left));
        assert_eq!(side_at(0.9, 0.5), Some(Dir::Right));
        assert_eq!(side_at(0.5, 0.05), Some(Dir::Up));
        assert_eq!(side_at(0.5, 0.95), Some(Dir::Down));
        assert_eq!(side_at(0.05, 0.2), Some(Dir::Left));
        assert_eq!(side_at(0.2, 0.05), Some(Dir::Up));
    }

    #[test]
    fn a_point_in_the_middle_drops_nowhere() {
        assert_eq!(side_at(0.5, 0.5), None);
        assert_eq!(side_at(0.31, 0.69), None);
    }

    #[test]
    fn a_press_is_no_drag_until_the_pointer_moves_far_enough() {
        let mut m = WindowMove::new(1, (100.0, 100.0));
        m.point((102.0, 101.0), Some(DropAt::Tab(0)));
        assert!(!m.dragging);
        assert_eq!(m.over, None);
        m.point((110.0, 100.0), Some(DropAt::Tab(0)));
        assert!(m.dragging);
        assert_eq!(m.over, Some(DropAt::Tab(0)));
        m.point((101.0, 100.0), None);
        assert!(m.dragging, "a drag stays one back near the press");
        assert_eq!(m.over, None);
    }

    #[test]
    fn a_window_dropped_onto_a_tab_goes_beside_its_focused_window() {
        let mut c = C::default();
        c.open("a");
        c.open_tab("b");
        c.split(Axis::Row, "c");
        let b = id_of(&c, "b");
        c.drop_window(b, DropAt::Tab(0));
        assert_eq!(c.current_index(), 0);
        let shown: Vec<&str> = c
            .current()
            .unwrap()
            .windows()
            .iter()
            .map(|w| w.shows)
            .collect();
        assert_eq!(shown, ["a", "b"]);
    }

    #[test]
    fn a_window_dropped_onto_its_own_tab_stays() {
        let mut c = C::default();
        c.open("a");
        c.split(Axis::Row, "b");
        let a = id_of(&c, "a");
        c.drop_window(a, DropAt::Tab(0));
        let shown: Vec<&str> = c
            .current()
            .unwrap()
            .windows()
            .iter()
            .map(|w| w.shows)
            .collect();
        assert_eq!(shown, ["a", "b"]);
    }

    #[test]
    fn a_window_dropped_onto_a_side_goes_there() {
        let mut c = C::default();
        c.open("a");
        c.open_tab("b");
        let (a, b) = (id_of(&c, "a"), id_of(&c, "b"));
        c.drop_window(b, DropAt::Side(a, Dir::Down));
        assert_eq!(c.tabs().len(), 1);
        let shown: Vec<&str> = c
            .current()
            .unwrap()
            .windows()
            .iter()
            .map(|w| w.shows)
            .collect();
        assert_eq!(shown, ["a", "b"]);
    }
}
