//! Instance-owned orchestration state and control services.

pub mod store;

pub mod actor;
pub mod cli;
pub mod client;
pub mod protocol;
pub mod service;

/// The control endpoint shares the terminal daemon's instance ownership.
pub fn root() -> std::path::PathBuf {
    crate::paths::instance_state().join("control")
}
