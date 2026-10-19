//! Sessions kept in a felis daemon, their screens as styled runs for
//! drawing, and keys as the daemon takes them.

mod daemon;
mod input;
mod palette;
mod pane;
mod screen;
pub mod tags;

pub use daemon::{Daemon, Error, Result, Session, Spawn, Status};
pub use input::Key;
pub use palette::{Palette, Rgb};
pub use pane::{Drawn, Pane, Picture, Size};
pub use screen::{Cursor, CursorShape, Line, Part, Run, Screen, Style, block, set_apart};
