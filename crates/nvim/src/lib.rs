//! Neovim embedded for writing prompts. Its UI protocol is read into the same
//! screen shape the terminals produce, with the completion menu, the command
//! line and messages kept apart so the window can draw them outside the grid.

mod embed;
mod keys;
mod reader;
mod ui;

pub use embed::{Candidate, Embedded, Event, PROMPT_APPNAME, SETUP};
pub use keys::notation;
pub use reader::{CONSOLE_APPNAME, Mark, READER};
pub use ui::{Cmdline, Popup, PopupItem, UiState};
