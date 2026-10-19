//! Where adroit looks for things on disk.

use std::path::PathBuf;

pub fn home() -> PathBuf {
    std::env::var_os("HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| "/".into())
}

/// `$XDG_STATE_HOME`, or `~/.local/state`.
pub fn state_dir() -> PathBuf {
    std::env::var_os("XDG_STATE_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| home().join(".local").join("state"))
}

/// `$XDG_DATA_HOME`, or `~/.local/share`.
pub fn data_dir() -> PathBuf {
    std::env::var_os("XDG_DATA_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| home().join(".local").join("share"))
}

/// Where scratch sessions run, each in a directory of its own:
/// `scratch` in adroit's folder of the data directory.
pub fn scratch_dir() -> PathBuf {
    data_dir().join("adroit").join("scratch")
}

/// adroit's configuration folder: `$XDG_CONFIG_HOME/adroit`, or
/// `~/.config/adroit`.
pub fn config_dir() -> PathBuf {
    std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| home().join(".config"))
        .join("adroit")
}

/// Where the user's key bindings live: `$ADROIT_KEYS`, else `keys.toml` in
/// the configuration folder.
pub fn keys_path() -> PathBuf {
    if let Some(p) = std::env::var_os("ADROIT_KEYS") {
        return PathBuf::from(p);
    }
    config_dir().join("keys.toml")
}
