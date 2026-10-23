//! What an update asks the world to do, as values the window runs.

use adroit_protocol::Request;

#[derive(Debug, Clone, PartialEq)]
pub enum Effect {
    /// Send `Request` to the work host.
    Host(Request),
    /// Draw the window at this zoom.
    Zoom(f64),
}
