//! Instance-owned orchestration state and control services.

pub mod store;

pub mod actor;
pub mod bridge;
pub mod cli;
pub mod client;
pub mod protocol;
pub mod service;
pub mod ui_authorization;

pub(crate) async fn app_command(
    root: &std::path::Path,
    id: adroit_model::RequestId,
    actor: Option<adroit_model::SessionRef>,
    command: protocol::Command,
) -> Result<serde_json::Value, store::ControlError> {
    let path = root.join("state.sqlite3");
    tokio::task::spawn_blocking(move || {
        let mut store = store::Store::open(&path)?;
        service::app_request(&mut store, id, actor, command)
    })
    .await
    .map_err(|error| {
        store::ControlError::new(store::ErrorCode::StorageFailure, error.to_string())
    })?
}

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

pub mod models;

pub mod runtime;

pub mod dispatcher;

pub mod observer;

pub mod files;

pub mod capture;

pub mod envelope;

mod restart;

mod tools;
