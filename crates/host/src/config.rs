//! What the host reads of the user's configuration: `settings.toml` and
//! `keys.toml` in adroit's configuration folder, as they are on disk, and what
//! it makes of the settings.

use std::path::Path;

use adroit_protocol::{ConfigFile, ConfigWork, Configured, FileText, ThemeKind, ThemeRead};

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
        configured: Some(configured(
            &crate::settings::current(),
            crate::appearance::system(),
            None,
        )),
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

/// What the host makes of `settings` with the system in the appearance
/// `system`: where scratch sessions run, and the colours of the theme they
/// name for it, read again only when it is another than the one `last` made
/// of.
pub fn configured(settings: &Settings, system: ThemeKind, last: Option<&Configured>) -> Configured {
    let themes_dir = crate::settings::themes_dir();
    let name = settings.theme_in(system);
    let theme = match last {
        Some(last) if last.theme.name == name => last.theme.clone(),
        _ => ThemeRead {
            name: name.to_string(),
            colours: crate::themes::colours(name, &themes_dir),
        },
    };
    Configured {
        scratch: crate::settings::scratch_folder(settings, &crate::paths::home()),
        theme,
        themes: crate::themes::listed(&themes_dir),
        system,
        themes_dir,
        login_shell: crate::programs::login_shell(),
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
    fn the_theme_in_force_is_the_one_named_for_the_system_s_appearance_unless_one_is_chosen() {
        use crate::settings::Appearance;
        let both = Settings {
            light_theme: "Catppuccin Latte".into(),
            dark_theme: "Nord".into(),
            ..Settings::default()
        };
        for (appearance, system, name) in [
            (Appearance::System, ThemeKind::Light, "Catppuccin Latte"),
            (Appearance::System, ThemeKind::Dark, "Nord"),
            (Appearance::Light, ThemeKind::Dark, "Catppuccin Latte"),
            (Appearance::Dark, ThemeKind::Light, "Nord"),
        ] {
            let settings = Settings {
                appearance,
                ..both.clone()
            };
            let made = configured(&settings, system, None);
            assert_eq!(made.theme.name, name, "{appearance:?} in {system:?}");
            assert_eq!(made.system, system);
        }
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
            ..Default::default()
        };
        let mine = Settings {
            dark_theme: "Mine".into(),
            ..Settings::default()
        };
        let kept = configured(&mine, ThemeKind::Dark, Some(&last));
        assert_eq!(kept.theme, last.theme);
        assert_eq!(
            kept.scratch,
            crate::settings::scratch_folder(&mine, &crate::paths::home())
        );
        // The system turning light names the other theme.
        let read = configured(
            &Settings {
                light_theme: "Nord".into(),
                ..mine
            },
            ThemeKind::Light,
            Some(&last),
        );
        assert_eq!(read.theme.name, "Nord");
        assert!(matches!(
            read.theme.colours,
            adroit_protocol::Colours::Theme { .. }
        ));
    }
}
