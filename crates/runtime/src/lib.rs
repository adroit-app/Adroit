//! What the window runs: the daemon and the sessions and shells it keeps,
//! the editor and the window's socket, delegation between sessions, and what
//! is kept on disk from one run to the next.

pub mod alerts;
pub mod awake;
pub mod cli;
pub mod complete;
pub mod config;
pub mod console_read;
pub mod daemon;
pub mod delegation;
pub mod details;
pub mod editor;
pub mod env;
pub mod execution;
pub mod fonts;
pub mod layout;
pub mod pasted;
pub mod paths;
pub mod pictures;
pub mod processes;
pub mod programs;
pub mod scratch;
pub mod sessions;
pub mod settings;
pub mod standing;
pub mod start;
pub mod system_usage;
pub mod terminals;
pub mod themes;
pub mod watch;
pub mod window_socket;
pub mod work;
pub mod work_host;

use adroit_core::content;

pub use daemon::cli as daemon_cli;
pub use start::cli as session_cli;
pub use terminals::cli as terminal_cli;
pub use work_host::cli as host_cli;

#[cfg(test)]
#[path = "../tests/support/daemon.rs"]
mod native_test_daemon;
