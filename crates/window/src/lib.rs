//! The adroit window.

pub use adroit_core::actions;
mod alerts;
mod bridge;
mod build;
mod bundled_font;
mod clipboard;
use adroit_core::completion;
mod complete;
mod components;
mod console;
mod console_read;
mod content;
pub use adroit_core::file_icons;
use adroit_core::fonts;
pub use adroit_model::files;
mod details;
pub mod editor;
mod feed;
mod hosts;
mod icons;
mod input;
mod installed_fonts;
pub use adroit_core::keymap;
mod keys;
use adroit_core::keys_by_action;
use adroit_core::keys_edit;
use adroit_core::keys_help;
mod keys_help_view;
mod keys_pane;
mod layout;
mod link;
mod logins;
pub mod menu_bar;
mod picker;
pub use adroit_core::tree::nav;
use adroit_model::paths;
mod panes;
mod pictures;
pub mod prompts;
pub mod rename;
use adroit_core::settings;
mod settings_window;
mod shell;
mod style;
mod themes;
mod tree;
mod view;
pub mod viewers;
pub use adroit_core::zoom;
mod window_socket;

pub use build::Build;
pub use shell::launch;
pub use window_socket::open_window;

#[cfg(test)]
#[path = "../../host/tests/support/daemon.rs"]
mod native_test_daemon;
