//! What can happen to the window: what the user does, what the window is
//! told, and what the work host says.

use adroit_protocol::Event;

use crate::details::{self, Sheet};
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

    // The details panel.
    /// The window shows what the details panel reads by.
    DetailsShown(Box<details::Shown>),
    /// A sheet was chosen.
    DetailsSheet(Sheet),
    /// The sheet this far along was asked for.
    DetailsSheetStep(isize),
    /// A key was pressed on the panel.
    DetailsKey(details::Key),
    /// A row of the overview was clicked.
    DetailsClick(usize),
    /// A row of the changes or the log was clicked.
    DetailsOpenRow(usize),
    /// A row of the overview was picked to open a menu on.
    DetailsSelect(usize),
    /// Back out of the row opened.
    DetailsBack,
    /// What the panel shows changed by the window's hand; read it again now.
    DetailsReadAgain,
}
