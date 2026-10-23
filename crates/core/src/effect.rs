//! What an update asks the world to do, as values the window runs.

use std::time::Duration;

use adroit_protocol::Request;

use crate::layout::Layout;

#[derive(Debug, Clone, PartialEq)]
pub enum Effect {
    /// Send `Request` to the work host.
    Host(Request),
    /// Draw the window at this zoom.
    Zoom(f64),

    // The chrome.
    /// Keep this layout of the side panels for the next run.
    KeepLayout(Layout),
    /// Tell the window with [`crate::msg::Msg::NoticeLapsed`], `after` this
    /// long, that the notice numbered `notice` saying `text` was shown.
    LapseNotice {
        notice: u64,
        text: String,
        after: Duration,
    },
}
