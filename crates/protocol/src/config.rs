//! What the host reads of the user's configuration: the settings and the
//! key bindings, as the files hold them, read again as they change, and what
//! it makes of the settings: where scratch sessions run, the theme's colours.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::work::set;

/// What a file of the configuration held when the host read it.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum FileText {
    /// There is no such file.
    #[default]
    Missing,
    Read {
        text: String,
    },
    /// The file is there but cannot be read, for this reason.
    Unreadable {
        error: String,
    },
}

/// A file of the configuration, where it is and what it held.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ConfigFile {
    pub path: PathBuf,
    pub text: FileText,
}

/// The colours of a theme, as the host read them.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Colours {
    /// adroit's own.
    #[default]
    Own,
    /// As a Ghostty theme file writes them.
    Theme { text: String },
    /// They cannot be had, for this reason.
    Unreadable { error: String },
}

/// Whether a theme is drawn light text on a dark ground or the other way,
/// and which of the two the system's appearance is in.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ThemeKind {
    Light,
    #[default]
    Dark,
}

/// A theme there is to choose from, and its kind.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ThemeListed {
    pub name: String,
    pub kind: ThemeKind,
}

/// The theme the settings name for the appearance in force, and its colours.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ThemeRead {
    pub name: String,
    pub colours: Colours,
}

/// What the host made of the settings it read.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Configured {
    /// Where scratch sessions run.
    pub scratch: PathBuf,
    pub theme: ThemeRead,
    /// The themes there are to choose from, with their kinds.
    pub themes: Vec<ThemeListed>,
    /// The appearance the system was in when the host read it.
    pub system: ThemeKind,
    /// Where the user's own themes go.
    pub themes_dir: PathBuf,
    /// The shell a shell runs when the settings name none.
    pub login_shell: String,
    /// The host's home directory, where the picker's directory tree starts;
    /// empty from a host that does not say.
    #[serde(default)]
    pub home: PathBuf,
}

impl Configured {
    /// The kind of the theme `name`, as listed; one not listed counts as
    /// dark, as the host counts a theme it cannot read.
    pub fn kind_of(&self, name: &str) -> ThemeKind {
        self.themes
            .iter()
            .find(|t| t.name == name)
            .map_or(ThemeKind::Dark, |t| t.kind)
    }
}

/// What the host read of the configuration; none of a file until it is
/// read.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct ConfigWork {
    /// `settings.toml`.
    pub settings: Option<ConfigFile>,
    /// `keys.toml`.
    pub keys: Option<ConfigFile>,
    /// What the host made of the settings; none until it read them.
    pub configured: Option<Configured>,
}

/// One change to what the host read of the configuration.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ConfigChange {
    Settings { file: ConfigFile },
    Keys { file: ConfigFile },
    Configured { configured: Configured },
}

impl ConfigWork {
    /// What was read with `change` made; whether that changed it.
    pub fn apply(&mut self, change: ConfigChange) -> bool {
        match change {
            ConfigChange::Settings { file } => set(&mut self.settings, Some(file)),
            ConfigChange::Keys { file } => set(&mut self.keys, Some(file)),
            ConfigChange::Configured { configured } => set(&mut self.configured, Some(configured)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn file(text: FileText) -> ConfigFile {
        ConfigFile {
            path: "/c/settings.toml".into(),
            text,
        }
    }

    #[test]
    fn each_file_read_replaces_the_one_before_and_the_same_text_is_no_news() {
        let mut work = ConfigWork::default();
        let read = |text: &str| ConfigChange::Settings {
            file: file(FileText::Read { text: text.into() }),
        };
        assert!(work.apply(read("zoom = 1.25\n")));
        assert!(!work.apply(read("zoom = 1.25\n")));
        assert!(work.apply(read("zoom = 1.5\n")));
        assert!(work.apply(ConfigChange::Keys {
            file: file(FileText::Missing),
        }));
        assert_eq!(
            work.settings.map(|f| f.text),
            Some(FileText::Read {
                text: "zoom = 1.5\n".into()
            })
        );
        assert_eq!(work.keys.map(|f| f.text), Some(FileText::Missing));
    }

    #[test]
    fn a_theme_s_kind_is_as_listed_and_one_not_listed_counts_as_dark() {
        let configured = Configured {
            themes: vec![
                ThemeListed {
                    name: "Latte".into(),
                    kind: ThemeKind::Light,
                },
                ThemeListed {
                    name: "Nord".into(),
                    kind: ThemeKind::Dark,
                },
            ],
            ..Default::default()
        };
        assert_eq!(configured.kind_of("Latte"), ThemeKind::Light);
        assert_eq!(configured.kind_of("Nord"), ThemeKind::Dark);
        assert_eq!(configured.kind_of("Gone"), ThemeKind::Dark);
    }

    #[test]
    fn what_the_host_made_of_the_settings_replaces_what_it_made_before() {
        let mut work = ConfigWork::default();
        let made = |name: &str| ConfigChange::Configured {
            configured: Configured {
                scratch: "/s".into(),
                theme: ThemeRead {
                    name: name.into(),
                    colours: Colours::Own,
                },
                ..Default::default()
            },
        };
        assert!(work.apply(made("Adroit")));
        assert!(!work.apply(made("Adroit")));
        assert!(work.apply(made("Nord")));
        assert_eq!(work.configured.map(|c| c.theme.name), Some("Nord".into()));
    }
}
