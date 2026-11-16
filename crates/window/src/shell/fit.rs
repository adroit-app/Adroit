//! The window's first size, kept on the screen it opens on.

use dioxus::desktop::LogicalSize;
use dioxus::prelude::use_hook;

/// The size the window asks for when it opens.
pub(super) const OPENING: LogicalSize<f64> = LogicalSize::new(1440.0, 900.0);

/// What the system's own frame takes from the screen beside the page: the
/// window's title bar and the taskbar or panel.
const FRAME: LogicalSize<f64> = LogicalSize::new(16.0, 120.0);

/// Whether a window `wanted` large, framed, fits on a screen `screen` large;
/// one that does not is better maximized than cut off by the screen's edge.
pub(super) fn fits(wanted: LogicalSize<f64>, screen: LogicalSize<f64>) -> bool {
    wanted.width + FRAME.width <= screen.width && wanted.height + FRAME.height <= screen.height
}

/// Maximize the window when the size it opened at does not fit on its
/// screen. macOS keeps a window within the screen itself.
pub(super) fn use_opening_fit() {
    use_hook(|| {
        if cfg!(target_os = "macos") {
            return;
        }
        let window = dioxus::desktop::window();
        let Some(monitor) = window.current_monitor() else {
            return;
        };
        let screen = monitor.size().to_logical::<f64>(monitor.scale_factor());
        if !fits(OPENING, screen) {
            window.set_maximized(true);
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_window_is_maximized_only_on_a_screen_its_frame_does_not_fit() {
        let table = [
            (1280.0, 800.0, false),
            (1440.0, 900.0, false),
            (1920.0, 1080.0, true),
            (1456.0, 1020.0, true),
            (1455.0, 1020.0, false),
            (1456.0, 1019.0, false),
        ];
        for (width, height, fit) in table {
            assert_eq!(
                fits(OPENING, LogicalSize::new(width, height)),
                fit,
                "{width}x{height}"
            );
        }
    }
}
