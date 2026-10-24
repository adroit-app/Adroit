//! The work host: the work it holds and serves over its socket, the
//! terminal host it starts and the sessions and shells it starts there,
//! delegation between sessions, the commands that ask it, and what is kept
//! on disk from one run to the next.

mod appearance;
mod asks;
pub mod awake;
pub mod cli;
pub mod compose;
pub mod config;
pub mod daemon;
pub mod delegation;
pub mod details;
pub mod drafts;
pub mod editor;
pub mod env;
pub mod execution;
mod held;
mod jobs;
pub mod pasted;
pub mod paths;
pub mod pictures;
pub mod processes;
pub mod programs;
pub mod scratch;
pub mod serve;
pub mod sessions;
pub mod settings;
pub mod standing;
pub mod start;
pub mod system_usage;
mod terminal_host;
pub mod terminals;
#[cfg(test)]
mod testing;
pub mod themes;
pub mod watch;
pub mod work;
pub mod work_host;

use adroit_core::content;

pub use daemon::cli as daemon_cli;
pub use start::cli as session_cli;
pub use terminals::cli as terminal_cli;
pub use work_host::cli as host_cli;
