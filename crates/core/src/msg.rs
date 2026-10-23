//! What can happen to the window: what the user does, what the window is
//! told, and what the work host says.

use adroit_protocol::Event;

use crate::zoom;

#[derive(Debug, Clone, PartialEq)]
pub enum Msg {
    /// The window opened, with the zoom the settings name.
    Opened { zoom: f64 },
    /// A key asked to zoom.
    Zoom(zoom::Step),
    /// The settings were read again, naming this zoom.
    ZoomSetting(f64),
    /// The work host said `Event`.
    Host(Event),
}
