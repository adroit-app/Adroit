//! A window dragged with the mouse, by its strip or by its tab: where it
//! would go were it let go now, and moving it there once it is. A tab
//! dragged among the tabs takes another place among them instead.

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
    /// The tab dragged, to that place among the tabs.
    Place(usize),
}

/// A window pressed to be dragged, and, once moved far enough to be dragged,
/// where it would go.
#[derive(Debug, Clone, PartialEq)]
pub struct WindowMove {
    pub window: WindowId,
    /// The place among the tabs of the tab pressed, when the window is
    /// dragged by its tab.
    pub tab: Option<usize>,
    /// Where the pointer pressed it, in pixels.
    from: (f64, f64),
    /// Whether the pointer moved far enough from the press to drag it.
    pub dragging: bool,
    pub over: Option<DropAt>,
}

impl WindowMove {
    pub fn new(window: WindowId, tab: Option<usize>, from: (f64, f64)) -> Self {
        WindowMove {
            window,
            tab,
            from,
            dragging: false,
            over: None,
        }
    }

    /// The pointer at `at`, over `over`; a tab dragged over a tab would
    /// take its place.
    pub fn point(&mut self, at: (f64, f64), over: Option<DropAt>) {
        let (dx, dy) = (at.0 - self.from.0, at.1 - self.from.1);
        if !self.dragging && dx.hypot(dy) >= THRESHOLD {
            self.dragging = true;
        }
        if self.dragging {
            self.over = match (self.tab, over) {
                (Some(_), Some(DropAt::Tab(at))) => Some(DropAt::Place(at)),
                (_, over) => over,
            };
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
    /// Move what `moving` drags where it would go, if anywhere: the window
    /// as [`Content::place`] moves it, into a tab beside its focused window,
    /// right of it, and never into the tab it is in; the tab it was pressed
    /// by to another place.
    pub fn drop_moved(&mut self, moving: &WindowMove) {
        let (id, Some(to)) = (moving.window, moving.over.filter(|_| moving.dragging)) else {
            return;
        };
        match to {
            DropAt::Place(at) => {
                if let Some(from) = moving.tab {
                    self.move_tab(from, at);
                }
            }
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

    /// The window `id`, pressed by the tab at `tab` if any, dragged over `to`.
    fn dragged(id: WindowId, tab: Option<usize>, to: DropAt) -> WindowMove {
        let mut m = WindowMove::new(id, tab, (0.0, 0.0));
        m.point((100.0, 0.0), Some(to));
        m
    }

    #[test]
    fn a_tab_dragged_over_a_tab_would_take_its_place_and_a_window_would_go_into_it() {
        assert_eq!(
            dragged(1, Some(0), DropAt::Tab(2)).over,
            Some(DropAt::Place(2))
        );
        assert_eq!(dragged(1, None, DropAt::Tab(2)).over, Some(DropAt::Tab(2)));
        let a_side = DropAt::Side(2, Dir::Left);
        assert_eq!(dragged(1, Some(0), a_side).over, Some(a_side));
    }

    #[test]
    fn a_tab_dropped_onto_another_place_moves_there() {
        let mut c = C::default();
        c.open("a");
        c.open_tab("b");
        c.open_tab("c");
        let a = id_of(&c, "a");
        c.drop_moved(&dragged(a, Some(0), DropAt::Tab(2)));
        let order: Vec<&str> = c.tabs().iter().map(|t| t.focused().shows).collect();
        assert_eq!(order, ["b", "c", "a"]);
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
        let mut m = WindowMove::new(1, None, (100.0, 100.0));
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
        c.drop_moved(&dragged(b, None, DropAt::Tab(0)));
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
        c.drop_moved(&dragged(a, None, DropAt::Tab(0)));
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
        c.drop_moved(&dragged(b, None, DropAt::Side(a, Dir::Down)));
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
