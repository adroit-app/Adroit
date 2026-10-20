//! The adroit window.

mod agents;
mod alerts;
mod app;
mod awake;
pub mod candidates;
pub mod cli;
mod complete;
mod completion;
mod console;
pub mod console_keys;
mod console_read;
pub mod control;
mod daemon;
mod details;
pub mod editor;
pub mod env;
pub mod execution;
pub mod filer;
pub mod focus;
mod fuzzy;
mod graph;
mod icons;
mod image_window;
pub mod keymap;
mod keys;
mod keys_edit;
mod keys_pane;
mod layout;
mod leader_details;
pub mod leader_navigation;
pub mod menu;
pub mod menu_bar;
pub mod nav;
mod pasted;
mod paths;
pub mod picker;
mod picker_view;
mod pictures;
mod plugin;
mod processes;
mod programs;
pub mod prompts;
pub mod rename;
pub mod scratch;
mod sessions;
pub mod settings;
mod settings_window;
mod start;
pub mod status;
mod style;
mod subagent_view;
mod system_usage;
pub mod tabs;
pub mod terminals;
pub mod themes;
mod view;
mod window_socket;
pub mod zoom;

pub use app::launch;
pub use daemon::cli as daemon_cli;
pub use start::cli as session_cli;
pub use terminals::cli as terminal_cli;

mod roles;

pub mod workareas;

#[cfg(test)]
#[path = "../tests/support/daemon.rs"]
mod native_test_daemon;
