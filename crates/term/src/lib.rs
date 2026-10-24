//! Sessions kept in a felis daemon, their screens as styled runs for
//! drawing, and keys as the daemon takes them.

mod daemon;
mod input;
mod palette;
mod pane;
pub mod picture;
mod presence;
mod screen;
pub mod tags;

pub use daemon::{
    Daemon, Error, Result, Session, Spawn, Status, detached, listening, own_dir, stop_listening,
};
pub use input::{Input, Key};
pub use palette::{Palette, Rgb};
pub use pane::{Drawn, Pane, Picture, Size};
pub use presence::Presence;
pub use screen::{Cursor, CursorShape, Line, Part, Run, Screen, Style, block, set_apart};

/// The versions of the terminal protocol this build speaks: felis's wire
/// format at the revision adroit is built with, and the session tags adroit
/// keeps in it. A version is added, and one no longer spoken dropped, only
/// when what is sent changes shape, so builds that share one work together.
pub const SPEAKS: &[u32] = &[1];
