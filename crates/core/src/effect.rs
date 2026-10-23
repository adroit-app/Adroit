//! What an update asks the world to do, as values the window runs.

use adroit_protocol::Request;

use crate::details::{Scroll, Shot};

#[derive(Debug, Clone, PartialEq)]
pub enum Effect {
    /// Send `Request` to the work host.
    Host(Request),
    /// Draw the window at this zoom.
    Zoom(f64),

    // The details panel.
    /// Open `target`, an address or a file, in the app macOS opens it with.
    Open(String),
    /// Show the file at this path in the Finder.
    Reveal(String),
    /// Put `text` on the clipboard and say `done`.
    Copy { text: String, done: String },
    /// Scroll the details panel's sheet.
    ScrollDetails(Scroll),
    /// Show the session by this id in the middle.
    ShowSession(String),
    /// Show an image of the overview in a window of its own; one the console
    /// drew is the console's of `session`.
    ShowShot { shot: Shot, session: String },
    /// Tell the user this in the status line.
    Notice(String),
}
