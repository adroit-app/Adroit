//! Neovim embedded for writing prompts. Its UI protocol is read into the same
//! screen shape the terminals produce, with the completion menu, the command
//! line and messages kept apart so the window can draw them outside the grid.

mod embed;
mod keys;
mod lua;
mod reader;
mod ui;

pub use embed::{Candidate, Embedded, Event, View};
pub use keys::notation;
pub use lua::{program, version};
pub use reader::Mark;
pub use ui::{Cmdline, Popup, PopupItem, UiState};
