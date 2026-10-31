//! The window's stylesheet, and the height of a terminal row: as tall as a
//! line of the console's font, measured each time the font changes.

use dioxus::prelude::*;

/// A line of the built-in font, until the font is measured.
pub const DEFAULT_CELL_HEIGHT: f64 = 17.0;

/// How tall a line of the console is, handed to the components that draw
/// cells through the context.
#[derive(Clone, Copy)]
pub struct CellHeight(pub Signal<f64>);

/// How tall a line of the console is, drawn again when it is measured anew.
pub fn use_cell_height() -> f64 {
    use_context::<CellHeight>().0()
}

/// The main window's stylesheet, an area to a file, in the order the
/// cascade needs.
pub const CSS: &str = concat!(
    include_str!("../assets/css/tokens.css"),
    include_str!("../assets/css/base.css"),
    include_str!("../assets/css/chrome.css"),
    include_str!("../assets/css/tree.css"),
    include_str!("../assets/css/content.css"),
    include_str!("../assets/css/console.css"),
    include_str!("../assets/css/prompt.css"),
    include_str!("../assets/css/details.css"),
    include_str!("../assets/css/footer.css"),
    include_str!("../assets/css/controls.css"),
    include_str!("../assets/css/launch.css"),
    include_str!("../assets/css/picker.css"),
    include_str!("../assets/css/keys-help.css"),
    include_str!("../assets/css/adding.css"),
);

/// The Settings window's stylesheet, on the main window's colours.
pub const SETTINGS_CSS: &str = concat!(
    include_str!("../assets/css/tokens.css"),
    include_str!("../assets/css/settings.css"),
);
