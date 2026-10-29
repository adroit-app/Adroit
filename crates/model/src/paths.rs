//! Where adroit keeps things on disk, and paths as the window writes them
//! for people to read.

use std::path::{Path, PathBuf};

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

/// The folder of an instance's `state` that keeps what serving it to other
/// machines needs: the gateway's key, its paired clients, the services' logs.
pub fn remote_dir(state: &std::path::Path) -> PathBuf {
    state.join("remote")
}

/// `$ADROIT_DAEMON`, or `felis/daemon.sock` in the instance's folder: where
/// the terminal host listens.
pub fn terminal_host_socket() -> PathBuf {
    std::env::var_os("ADROIT_DAEMON")
        .map(PathBuf::from)
        .unwrap_or_else(|| instance_state().join("felis").join("daemon.sock"))
}

/// `$ADROIT_HOST`, or `host/host.sock` in the instance's folder: where the
/// work host listens.
pub fn work_host_socket() -> PathBuf {
    std::env::var_os("ADROIT_HOST")
        .map(PathBuf::from)
        .unwrap_or_else(|| instance_state().join("host").join("host.sock"))
}

/// `$ADROIT_WINDOWS`, or `windows` in the instance's folder `dir`: where
/// each window of the instance answers, and keeps what it keeps.
pub fn windows_dir(dir: &Path) -> PathBuf {
    std::env::var_os("ADROIT_WINDOWS")
        .map(PathBuf::from)
        .unwrap_or_else(|| dir.join("windows"))
}

/// Where the window numbered `number` answers, in the windows' folder
/// `windows`.
pub fn window_socket(windows: &Path, number: u32) -> PathBuf {
    windows.join(format!("{number}.sock"))
}

/// Where the window numbered `number` keeps how it shows the work from one
/// run to the next, in the windows' folder `windows`.
pub fn window_state(windows: &Path, number: u32) -> PathBuf {
    windows.join(number.to_string())
}

/// What `adroit editor` hands the user's own editor, `$ADROIT_USER_EDITOR`,
/// when the window did not ask for the text: the shell runs it, so it may
/// carry arguments of its own.
pub const USER_EDITOR: &str = "ADROIT_USER_EDITOR";

/// Where the editor script for the windows in the folder `windows` lives:
/// beside the folder, so the windows of other instances do not share one.
pub fn editor_script(windows: &Path) -> PathBuf {
    windows.with_extension("editor")
}

/// `path` with the home directory written as `~`.
pub fn tilde(path: &Path, home: &Path) -> String {
    match path.strip_prefix(home) {
        Ok(rest) if rest.as_os_str().is_empty() => "~".into(),
        Ok(rest) => format!("~/{}", rest.display()),
        Err(_) => path.display().to_string(),
    }
}

#[cfg(test)]
mod tests {
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

    #[test]
    fn each_folder_of_windows_has_a_script_of_its_own_beside_it() {
        assert_eq!(
            editor_script(Path::new("/run/adroit/windows")),
            Path::new("/run/adroit/windows.editor")
        );
        assert_ne!(
            editor_script(Path::new("/tmp/one/windows")),
            editor_script(Path::new("/tmp/two/windows"))
        );
    }

    #[test]
    fn each_window_answers_and_keeps_its_state_under_its_number() {
        let windows = Path::new("/s/adroit/windows");
        assert_eq!(
            window_socket(windows, 2),
            Path::new("/s/adroit/windows/2.sock")
        );
        assert_eq!(window_state(windows, 2), Path::new("/s/adroit/windows/2"));
        assert_ne!(window_socket(windows, 1), window_socket(windows, 2));
    }

    #[test]
    fn the_home_directory_is_written_as_a_tilde() {
        let home = Path::new("/Users/me");
        assert_eq!(tilde(Path::new("/Users/me/src/x"), home), "~/src/x");
        assert_eq!(tilde(Path::new("/Users/me"), home), "~");
        assert_eq!(tilde(Path::new("/Users/meow"), home), "/Users/meow");
        assert_eq!(tilde(Path::new("/opt/x"), home), "/opt/x");
    }
}
