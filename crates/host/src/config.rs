//! What the host reads of the user's configuration: `settings.toml` and
//! `keys.toml` in adroit's configuration folder, as they are on disk, and what
//! it makes of the settings.

use std::path::Path;

use adroit_protocol::{ConfigFile, ConfigWork, Configured, FileText, ThemeKind, ThemeRead};

use crate::settings::Settings;

/// Make `text` what the configuration file `path` holds, replaced whole, its
/// folder made when it is missing. A file the user keeps elsewhere and links
/// to is written where it is kept, so the link stays, and a file keeps the
/// mode it has.
pub(crate) fn write(path: &Path, text: &str) -> std::io::Result<()> {
    use adroit_sys::fs::MetadataExt;
    let kept = match std::fs::symlink_metadata(path) {
        Ok(metadata) if metadata.is_symlink() => adroit_sys::fs::canonicalize(path)?,
        _ => path.to_path_buf(),
    };
    if let Some(dir) = kept.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let mode = std::fs::metadata(&kept).map_or(0o644, |metadata| metadata.mode() & 0o7777);
    adroit_sys::fs::replace(&kept, text, mode)
}

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
        home: crate::paths::home(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_configuration_file_is_written_whole_with_its_folder_and_leaves_nothing_beside_it() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("config").join("settings.toml");

        write(&path, "zoom = 1.0\n").unwrap();
        write(&path, "zoom = 1.25\n").unwrap();

        assert_eq!(std::fs::read_to_string(&path).unwrap(), "zoom = 1.25\n");
        let beside: Vec<_> = std::fs::read_dir(path.parent().unwrap())
            .unwrap()
            .map(|entry| entry.unwrap().file_name())
            .collect();
        assert_eq!(beside, ["settings.toml"]);
    }

    #[cfg(unix)]
    #[test]
    fn a_configuration_file_keeps_the_mode_it_has() {
        use adroit_sys::fs::MetadataExt;
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("keys.toml");
        write(&path, "").unwrap();
        assert_eq!(std::fs::metadata(&path).unwrap().mode() & 0o777, 0o644);
        adroit_sys::fs::set_mode(&path, 0o600).unwrap();

        write(&path, "[tree]\n").unwrap();

        assert_eq!(std::fs::metadata(&path).unwrap().mode() & 0o777, 0o600);
    }

    #[cfg(unix)]
    #[test]
    fn a_configuration_file_kept_elsewhere_through_a_link_is_written_where_it_is_kept() {
        let directory = tempfile::tempdir().unwrap();
        let kept = directory.path().join("dotfiles").join("settings.toml");
        std::fs::create_dir(kept.parent().unwrap()).unwrap();
        std::fs::write(&kept, "zoom = 1.0\n").unwrap();
        let path = directory.path().join("settings.toml");
        std::os::unix::fs::symlink(&kept, &path).unwrap();

        write(&path, "zoom = 1.25\n").unwrap();

        assert!(std::fs::symlink_metadata(&path).unwrap().is_symlink());
        assert_eq!(std::fs::read_to_string(&kept).unwrap(), "zoom = 1.25\n");
    }

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
