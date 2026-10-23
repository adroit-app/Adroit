//! What the host reads of the user's configuration: the settings and the
//! key bindings, as the files hold them, read again as they change.

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

/// What the host read of the configuration; none of a file until it is
/// read.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct ConfigWork {
    /// `settings.toml`.
    pub settings: Option<ConfigFile>,
    /// `keys.toml`.
    pub keys: Option<ConfigFile>,
}

/// One change to what the host read of the configuration.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ConfigChange {
    Settings { file: ConfigFile },
    Keys { file: ConfigFile },
}

impl ConfigWork {
    /// What was read with `change` made; whether that changed it.
    pub fn apply(&mut self, change: ConfigChange) -> bool {
        let (slot, file) = match change {
            ConfigChange::Settings { file } => (&mut self.settings, file),
            ConfigChange::Keys { file } => (&mut self.keys, file),
        };
        if slot.as_ref() == Some(&file) {
            return false;
        }
        *slot = Some(file);
        true
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
}
