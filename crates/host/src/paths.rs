//! Where adroit looks for things on disk.

use std::path::PathBuf;

pub use adroit_model::paths::*;

/// Where scratch sessions run, each in a directory of its own, as the
/// settings say.
pub fn scratch_dir() -> PathBuf {
    crate::settings::scratch_folder(&crate::settings::current(), &home())
}

/// Work files belong to the selected environment; service sockets stay shared.
pub fn config_dir() -> PathBuf {
    crate::environment::current()
        .map_or_else(adroit_model::paths::config_dir, |c| c.root.join("config"))
}

pub fn instance_state() -> PathBuf {
    crate::environment::current().map_or_else(adroit_model::paths::instance_state, |c| {
        c.root.join("state")
    })
}

pub fn data_dir() -> PathBuf {
    crate::environment::current()
        .map_or_else(adroit_model::paths::data_dir, |c| c.root.join("data"))
}

pub fn keys_path() -> PathBuf {
    if crate::environment::current().is_some() {
        config_dir().join("keys.toml")
    } else {
        adroit_model::paths::keys_path()
    }
}
