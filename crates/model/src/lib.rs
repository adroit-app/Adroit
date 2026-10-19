//! The data adroit shows, derived without I/O from what Claude Code reports,
//! what adroit's daemon reports of the shells it runs, and what git says
//! about the directories they run in.

mod inbox;
mod session;
mod shell;
mod tree;

pub use inbox::*;
pub use session::*;
pub use shell::*;
pub use tree::*;
