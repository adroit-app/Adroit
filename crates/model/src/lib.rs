//! The data adroit shows, derived without I/O from what Claude Code reports,
//! what adroit's daemon reports of the shells it runs, and what git says
//! about the directories they run in.

mod inbox;
mod leaders;
mod metrics;
mod models;
mod orchestration;
mod session;
mod terminal;
mod tree;

pub use inbox::*;
pub use leaders::*;
pub use metrics::*;
pub use models::*;
pub use orchestration::*;
pub use session::*;
pub use terminal::*;
pub use tree::*;
