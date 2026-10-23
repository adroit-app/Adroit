//! The panes of the sessions and shells the window shows: the hub that
//! holds them, how each is attached and sized, how sessions and shells
//! start, what is typed into them, and the images their consoles drew.

mod attach;
mod gallery;
mod hub;
mod size;
mod start;
mod typing;

pub(crate) use attach::*;
pub(crate) use gallery::*;
pub(crate) use hub::*;
pub(crate) use size::*;
pub(crate) use start::adroit_program;
