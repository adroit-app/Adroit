//! What the host reads of the user's configuration: `settings.toml` and
//! `keys.toml` in adroit's configuration folder, as they are on disk, and what
//! it makes of the settings.

use std::path::Path;

use adroit_protocol::{ConfigFile, ConfigWork, Configured, FileText, ThemeRead};

use crate::settings::Settings;

/// What `path` holds now.
pub fn file(path: &Path) -> ConfigFile {
    let text = match std::fs::read_to_string(path) {
        Ok(text) => FileText::Read { text },
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => FileText::Missing,
        Err(error) => FileText::Unreadable {
            error: error.to_string(),
        },
    };
    ConfigFile {
        path: path.to_path_buf(),
        text,
    }
}

/// Both files, read now, with what the host makes of the settings in force.
pub fn read() -> ConfigWork {
    ConfigWork {
        configured: Some(configured(&crate::settings::current(), None)),
        ..files()
    }
}

/// Both files, read now.
pub fn files() -> ConfigWork {
    ConfigWork {
        settings: Some(file(&crate::settings::path())),
        keys: Some(file(&crate::paths::keys_path())),
        configured: None,
    }
}

/// What the host makes of `settings`: where scratch sessions run, and the
/// colours of the theme they name, read again only when it is another than
/// the one `last` made of.
pub fn configured(settings: &Settings, last: Option<&Configured>) -> Configured {
    let theme = match last {
        Some(last) if last.theme.name == settings.theme => last.theme.clone(),
        _ => ThemeRead {
            name: settings.theme.clone(),
            colours: crate::themes::colours(&settings.theme, &crate::settings::themes_dir()),
        },
    };
    Configured {
        scratch: crate::settings::scratch_folder(settings, &crate::paths::home()),
        theme,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_file_is_read_missing_or_unreadable_as_it_is_on_disk() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("keys.toml");
        assert_eq!(file(&path).text, FileText::Missing);
        std::fs::write(&path, "[tree]\n").unwrap();
        assert_eq!(
            file(&path),
            ConfigFile {
                path: path.clone(),
                text: FileText::Read {
                    text: "[tree]\n".into()
                },
            }
        );
        assert!(matches!(
            file(directory.path()).text,
            FileText::Unreadable { .. }
        ));
    }

    #[test]
    fn the_theme_named_is_read_again_only_once_another_is_named() {
        let last = Configured {
            scratch: "/old".into(),
            theme: ThemeRead {
                name: "Mine".into(),
                colours: adroit_protocol::Colours::Theme {
                    text: "as read before".into(),
                },
            },
        };
        let mine = Settings {
            theme: "Mine".into(),
            ..Settings::default()
        };
        let kept = configured(&mine, Some(&last));
        assert_eq!(kept.theme, last.theme);
        assert_eq!(
            kept.scratch,
            crate::settings::scratch_folder(&mine, &crate::paths::home())
        );
        let nord = Settings {
            theme: "Nord".into(),
            ..Settings::default()
        };
        let read = configured(&nord, Some(&last));
        assert_eq!(read.theme.name, "Nord");
        assert!(matches!(
            read.theme.colours,
            adroit_protocol::Colours::Theme { .. }
        ));
    }
}
