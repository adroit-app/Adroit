//! What the host reads of the user's configuration: the settings and the
//! key bindings, as the files hold them, read again as they change, and what
//! it makes of the settings: where scratch sessions run, the theme's colours.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

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

/// The theme the settings name, and its colours.
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
    /// The themes there are to choose from, by name.
    pub themes: Vec<String>,
    /// Where the user's own themes go.
    pub themes_dir: PathBuf,
    /// The shell a shell runs when the settings name none.
    pub login_shell: String,
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
            ConfigChange::Settings { file } => replace(&mut self.settings, file),
            ConfigChange::Keys { file } => replace(&mut self.keys, file),
            ConfigChange::Configured { configured } => replace(&mut self.configured, configured),
        }
    }
}

/// `slot` holding `now`; whether it held something else.
fn replace<T: PartialEq>(slot: &mut Option<T>, now: T) -> bool {
    if slot.as_ref() == Some(&now) {
        return false;
    }
    *slot = Some(now);
    true
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
