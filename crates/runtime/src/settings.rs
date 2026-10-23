//! The window's settings, kept in `settings.toml` in adroit's configuration
//! folder, and told to every window as they change.

use std::path::{Path, PathBuf};
use std::sync::{LazyLock, RwLock};

use tokio::sync::broadcast;

use crate::paths::config_dir;
pub use adroit_core::settings::*;

/// Where scratch sessions run as `settings` say, `~` standing for `home`.
pub fn scratch_folder(settings: &Settings, home: &Path) -> PathBuf {
    settings.scratch_folder(home, &crate::paths::data_dir())
}

pub fn path() -> PathBuf {
    config_dir().join("settings.toml")
}

/// Where the user's own themes are, one file a theme, named as it is.
pub fn themes_dir() -> PathBuf {
    config_dir().join("themes")
}

fn read_path(path: &Path) -> Result<Settings, String> {
    match std::fs::read_to_string(path) {
        Ok(text) => Settings::parse(&text)
            .map_err(|error| format!("Cannot load {}: {error}", path.display())),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(Settings::default()),
        Err(error) => Err(format!("Cannot read {}: {error}", path.display())),
    }
}

/// Keep the UI available for repairing an invalid configuration, with
/// its error reported separately and saving blocked until it is fixed.
pub fn load() -> Settings {
    load_path(&path())
}

pub fn try_load() -> Result<Settings, String> {
    read_path(&path())
}

fn load_path(path: &Path) -> Settings {
    read_path(path).unwrap_or_else(|error| {
        eprintln!("adroit: {error}");
        Settings {
            delegation: DelegationSettings { max_started: 0 },
            ..Settings::default()
        }
    })
}

pub fn save(settings: &Settings) -> Result<(), String> {
    save_path(settings, &path())
}

fn save_path(settings: &Settings, path: &Path) -> Result<(), String> {
    read_path(path)
        .map_err(|error| format!("{error}. Fix the configuration file before saving settings"))?;
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    }
    let text = toml::to_string_pretty(settings).map_err(|e| e.to_string())?;
    std::fs::write(path, text).map_err(|e| e.to_string())
}

pub fn load_error() -> Option<String> {
    read_path(&path()).err()
}

/// The settings every window works by.
pub static SETTINGS: LazyLock<RwLock<Settings>> = LazyLock::new(|| RwLock::new(load()));

/// Told whenever [`SETTINGS`] changes.
pub static CHANGED: LazyLock<broadcast::Sender<()>> = LazyLock::new(|| broadcast::channel(16).0);

pub fn current() -> Settings {
    SETTINGS.read().map(|s| s.clone()).unwrap_or_default()
}

/// Take in the file's settings whenever it changes on disk, edited by hand or
/// by another program, looking every `every`.
pub fn watch(every: std::time::Duration) {
    let modified = || std::fs::metadata(path()).and_then(|m| m.modified()).ok();
    std::thread::spawn(move || {
        let mut seen = modified();
        loop {
            std::thread::sleep(every);
            let now = modified();
            if now == seen {
                continue;
            }
            seen = now;
            match read_path(&path()) {
                Ok(next) if next != current() => {
                    if let Ok(mut s) = SETTINGS.write() {
                        *s = next;
                    }
                }
                Err(error) => eprintln!("adroit: {error}"),
                _ => {}
            }
            let _ = CHANGED.send(());
        }
    });
}

/// Make `next` the settings: keep them for the next run and tell every window.
pub fn set(next: Settings) -> Result<(), String> {
    let next = next.sane();
    save(&next)?;
    if let Ok(mut s) = SETTINGS.write() {
        *s = next;
    }
    let _ = CHANGED.send(());
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn missing_settings_default_but_invalid_or_unreadable_settings_report_the_path() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("settings.toml");
        assert_eq!(read_path(&path).unwrap(), Settings::default());
        for text in ["[readers]\nmodel = 'haiku'\n", "zoom = "] {
            std::fs::write(&path, text).unwrap();
            let error = read_path(&path).unwrap_err();
            assert!(error.contains(&path.display().to_string()), "{error}");
        }
        let error = read_path(directory.path()).unwrap_err();
        assert!(
            error.contains(&directory.path().display().to_string()),
            "{error}"
        );
    }

    #[test]
    fn saving_refuses_invalid_existing_settings_until_the_file_is_fixed() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("settings.toml");
        let invalid = "[codex_readers]\nmodel = 'review-model'\n";
        std::fs::write(&path, invalid).unwrap();
        let changed = Settings {
            zoom: 1.25,
            ..Settings::default()
        };
        assert!(save_path(&changed, &path).is_err());
        assert_eq!(std::fs::read_to_string(&path).unwrap(), invalid);
        std::fs::write(&path, "zoom = 1.0\n").unwrap();
        save_path(&changed, &path).unwrap();
        assert_eq!(read_path(&path).unwrap(), changed);
    }

    #[test]
    fn an_invalid_configuration_never_turns_paused_starts_into_default_capacity() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("settings.toml");
        std::fs::write(&path, "[delegation]\nmax_started = 0\n").unwrap();
        assert_eq!(load_path(&path).delegation.max_started, 0);
        std::fs::write(&path, "[delegation]\nmax_started = 0\nzoom = ").unwrap();
        let recovery = load_path(&path).sane();
        assert_eq!(recovery.delegation.max_started, 0);
    }

    #[test]
    fn scratch_sessions_run_in_the_folder_the_settings_name_or_adroit_s_own() {
        let home = std::path::Path::new("/h");
        let at = |text: &str| scratch_folder(&Settings::parse(text).unwrap(), home);
        assert_eq!(
            at(""),
            crate::paths::data_dir().join("adroit").join("scratch")
        );
        assert_eq!(
            at("scratch_dir = \"~/notes/scratch\"\n"),
            PathBuf::from("/h/notes/scratch")
        );
        assert_eq!(at("scratch_dir = \"/tmp/s\"\n"), PathBuf::from("/tmp/s"));
    }
}
