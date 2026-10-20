//! Instance-owned orchestration state and control services.

pub mod store;

pub mod actor;
pub mod bridge;
pub mod cli;
pub mod client;
pub mod protocol;
pub mod service;

/// The control endpoint shares the terminal daemon's instance ownership.
pub fn root() -> std::path::PathBuf {
    crate::paths::instance_state().join("control")
}

pub mod scheduler;

/// An agent bridge keeps its launch endpoint even when the user changes settings.
pub fn session_root() -> std::path::PathBuf {
    std::env::var_os("ADROIT_CONTROL_ROOT")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(root)
}

pub mod managed;

pub mod runtime;

pub mod dispatcher;

pub mod observer;

pub mod files;
