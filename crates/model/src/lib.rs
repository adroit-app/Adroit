//! The data adroit shows, derived without I/O from what Claude Code reports,
//! what adroit's daemon reports of the shells it runs, and what git says
//! about the directories they run in.

pub mod config;
pub mod daemon;
mod inbox;
mod key;
mod launch;
pub mod links;
mod metrics;
mod models;
mod note;
pub mod paths;
mod process;
pub mod repository;
mod session;
pub mod subagents;
mod terminal;
#[cfg(any(test, feature = "testing"))]
pub mod testing;
pub mod time;
mod tree;
pub mod usage;

pub use inbox::*;
pub use key::*;
pub use launch::*;
pub use metrics::*;
pub use models::*;
pub use note::*;
pub use process::*;
pub use session::*;
pub use terminal::*;
pub use tree::*;
