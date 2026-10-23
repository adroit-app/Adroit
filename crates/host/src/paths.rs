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

/// Where scratch sessions run, each in a directory of its own, as the
/// settings say.
pub fn scratch_dir() -> PathBuf {
    crate::settings::scratch_folder(&crate::settings::current(), &home())
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

/// The instance every process started without one belongs to.
pub const DEFAULT_INSTANCE: &str = "default";

/// Where the instance a process belongs to is named.
pub const INSTANCE_ENV: &str = "ADROIT_INSTANCE";

/// Whether `name` can name an instance: letters, digits, `-` and `_`, short
/// enough that the sockets under its folder fit the length a socket's path
/// may have.
pub fn is_instance(name: &str) -> bool {
    (1..=32).contains(&name.len())
        && name
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
}

/// The instance this process belongs to: `$ADROIT_INSTANCE`, or the default
/// one.
pub fn instance() -> String {
    std::env::var(INSTANCE_ENV)
        .ok()
        .filter(|n| !n.is_empty())
        .unwrap_or_else(|| DEFAULT_INSTANCE.into())
}

/// The folder of `state` that `instance` keeps its daemon and state in: the
/// one adroit used before there were instances, for the default one.
pub fn instance_dir(state: &std::path::Path, instance: &str) -> PathBuf {
    let own = state.join("adroit");
    if instance == DEFAULT_INSTANCE {
        own
    } else {
        own.join("instances").join(instance)
    }
}

/// The folder this process's instance keeps its daemon and state in.
pub fn instance_state() -> PathBuf {
    instance_dir(&state_dir(), &instance())
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use super::*;

    #[test]
    fn the_default_instance_keeps_the_folder_adroit_always_used() {
        assert_eq!(
            instance_dir(Path::new("/s"), "default"),
            Path::new("/s/adroit")
        );
    }

    #[test]
    fn another_instance_keeps_a_folder_of_its_own() {
        assert_eq!(
            instance_dir(Path::new("/s"), "work"),
            Path::new("/s/adroit/instances/work")
        );
    }

    #[test]
    fn an_instance_is_named_by_letters_digits_dashes_and_underscores() {
        assert!(is_instance("work"));
        assert!(is_instance("dev-2_b"));
        assert!(is_instance(&"x".repeat(32)));
        assert!(!is_instance(""));
        assert!(!is_instance(&"x".repeat(33)));
        assert!(!is_instance("../up"));
        assert!(!is_instance("a b"));
        assert!(!is_instance("仕事"));
    }
}
