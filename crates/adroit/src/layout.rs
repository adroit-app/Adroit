//! How wide the side panels are and whether they are shown, kept across
//! runs of the window.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::paths::state_dir;

/// The narrowest a side panel is dragged to.
pub const MIN_SIDE: f64 = 160.0;
/// The narrowest the console is left by a side panel dragged wide, or by a
/// window too small for both panels: some sixty columns, as a program's
/// interface needs more than the panels beside it.
pub const MIN_CENTER: f64 = 480.0;
/// A side panel's width as designed, which a double-click on its edge brings back.
pub const SIDE: f64 = 300.0;

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Layout {
    pub sidebar: f64,
    pub details: f64,
    pub show_sidebar: bool,
    pub show_details: bool,
}

impl Default for Layout {
    fn default() -> Self {
        Self {
            sidebar: SIDE,
            details: SIDE,
            show_sidebar: true,
            show_details: true,
        }
    }
}

/// Which side panel an edge belongs to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Side {
    Sidebar,
    Details,
}

impl Layout {
    /// The layout with `side` dragged to `want` pixels wide, in a window
    /// `body` pixels wide: no narrower than [`MIN_SIDE`], and leaving the
    /// console at least [`MIN_CENTER`] beside the other panel.
    /// The other panel keeps the width it is drawn at, so the room one edge
    /// gives up goes to the console, not to the other panel growing back.
    pub fn dragged(self, side: Side, want: f64, body: f64) -> Self {
        let (sidebar, details) = self.drawn(body);
        let other = match side {
            Side::Sidebar if self.show_details => details,
            Side::Details if self.show_sidebar => sidebar,
            _ => 0.0,
        };
        let most = (body - other - MIN_CENTER).max(MIN_SIDE);
        let width = want.clamp(MIN_SIDE, most);
        match side {
            Side::Sidebar => Self {
                sidebar: width,
                details: if self.show_details {
                    details
                } else {
                    self.details
                },
                ..self
            },
            Side::Details => Self {
                details: width,
                sidebar: if self.show_sidebar {
                    sidebar
                } else {
                    self.sidebar
                },
                ..self
            },
        }
    }

    /// The widths the panels are drawn at in a window `body` pixels wide:
    /// as kept, or narrowed alike so the console keeps [`MIN_CENTER`], down
    /// to [`MIN_SIDE`]. The kept widths stay for a wider window.
    pub fn drawn(&self, body: f64) -> (f64, f64) {
        let shown = |on: bool, w: f64| if on { w } else { 0.0 };
        let total = shown(self.show_sidebar, self.sidebar) + shown(self.show_details, self.details);
        let room = body - MIN_CENTER;
        if total <= room || total <= 0.0 {
            return (self.sidebar, self.details);
        }
        let scale = room.max(0.0) / total;
        let fit = |on: bool, w: f64| if on { (w * scale).max(MIN_SIDE) } else { w };
        (
            fit(self.show_sidebar, self.sidebar),
            fit(self.show_details, self.details),
        )
    }

    pub fn toggled(self, side: Side) -> Self {
        match side {
            Side::Sidebar => Self {
                show_sidebar: !self.show_sidebar,
                ..self
            },
            Side::Details => Self {
                show_details: !self.show_details,
                ..self
            },
        }
    }

    fn path() -> PathBuf {
        state_dir().join("adroit").join("layout.json")
    }

    /// The layout kept from the last run, or as designed.
    pub fn load() -> Self {
        std::fs::read_to_string(Self::path())
            .ok()
            .and_then(|t| serde_json::from_str(&t).ok())
            .unwrap_or_default()
    }

    /// Keep the layout for the next run; failing that, it is only lost.
    pub fn save(&self) {
        let path = Self::path();
        if let Some(dir) = path.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        if let Ok(text) = serde_json::to_string(self) {
            let _ = std::fs::write(path, text);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_panel_is_dragged_between_its_floor_and_the_console_s() {
        let l = Layout::default();
        assert_eq!(l.dragged(Side::Sidebar, 420.0, 1400.0).sidebar, 420.0);
        assert_eq!(l.dragged(Side::Sidebar, 40.0, 1400.0).sidebar, MIN_SIDE);
        // 1400 less the details panel's 300 and the console's floor.
        assert_eq!(
            l.dragged(Side::Sidebar, 2000.0, 1400.0).sidebar,
            1400.0 - 300.0 - MIN_CENTER
        );
    }

    #[test]
    fn narrowing_one_panel_in_a_narrow_window_gives_its_room_to_the_console() {
        let body = 944.0;
        let l = Layout::default();
        let (_, details) = l.drawn(body);
        let after = l.dragged(Side::Sidebar, 180.0, body);
        assert_eq!(after.drawn(body), (180.0, details));
    }

    #[test]
    fn a_narrow_window_narrows_the_panels_before_the_console() {
        let l = Layout {
            sidebar: 300.0,
            details: 500.0,
            ..Layout::default()
        };
        assert_eq!(l.drawn(1400.0), (300.0, 500.0));
        // 1080 wide leaves the panels 600 beside a console of 480, shared as
        // they were.
        assert_eq!(l.drawn(1080.0), (225.0, 375.0));
        // A small screen's window still gives the console room for a
        // program's interface.
        assert_eq!(Layout::default().drawn(944.0), (232.0, 232.0));
        // Too narrow even for that, they stay at their floor.
        assert_eq!(l.drawn(400.0), (MIN_SIDE, MIN_SIDE));
        let one = l.toggled(Side::Sidebar);
        assert_eq!(one.drawn(760.0).1, 760.0 - MIN_CENTER);
    }

    #[test]
    fn a_hidden_panel_leaves_its_room_to_the_other() {
        let l = Layout::default().toggled(Side::Details);
        assert!(!l.show_details);
        assert_eq!(
            l.dragged(Side::Sidebar, 2000.0, 1400.0).sidebar,
            1400.0 - MIN_CENTER
        );
        assert!(l.toggled(Side::Details).show_details);
    }

    #[test]
    fn a_kept_layout_missing_a_field_takes_the_designed_one() {
        let l: Layout = serde_json::from_str(r#"{"sidebar":250}"#).unwrap();
        assert_eq!(l.sidebar, 250.0);
        assert_eq!(l.details, SIDE);
        assert!(l.show_details);
    }
}
