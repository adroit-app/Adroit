//! The data adroit shows, derived without I/O from what Claude Code reports,
//! what adroit's daemon reports of the shells it runs, and what git says
//! about the directories they run in.

pub mod config;
pub mod daemon;
mod inbox;
mod key;
mod launch;
mod metrics;
mod models;
pub mod paths;
mod session;
mod terminal;
#[cfg(any(test, feature = "testing"))]
pub mod testing;
pub mod time;
mod tree;

pub use inbox::*;
pub use key::*;
pub use launch::*;
pub use metrics::*;
pub use models::*;
pub use session::*;
pub use terminal::*;
pub use tree::*;
