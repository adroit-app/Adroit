//! The user's configuration as the window works by it: the settings and the
//! key bindings, taken in from the files as the host last read them.

use adroit_model::paths::tilde;
use adroit_protocol::{ConfigFile, ConfigWork, FileText};

use crate::chrome::status_line;
use crate::effect::Effect;
use crate::keymap::Keymap;
use crate::settings::Settings;
use crate::state::ViewState;

#[derive(Debug, Clone, Default, PartialEq)]
pub struct Config {
    /// The settings in force: those last read that could be had, or the
    /// settings as designed before any could.
    pub settings: Settings,
    /// Why the settings file cannot be had, while it cannot.
    pub settings_error: Option<String>,
    /// Why the key bindings file cannot be had, while it cannot.
    keys_error: Option<String>,
    /// The files as they were last taken in.
    taken: ConfigWork,
}

/// The files the host read, taken in once each time one changes: the
/// settings replace those in force, and the key bindings the chrome's. A
/// file that cannot be had leaves what is in force and says why, the key
/// bindings' in the status line until they read again.
pub(crate) fn take_in(state: &mut ViewState) -> Vec<Effect> {
    let Some(work) = state.host.work.as_ref() else {
        return Vec::new();
    };
    let config = &mut state.config;
    let mut effects = Vec::new();
    if work.config.settings != config.taken.settings {
        config.taken.settings = work.config.settings.clone();
        if let Some(file) = &config.taken.settings {
            match Settings::read(file) {
                Ok(settings) => {
                    config.settings = settings;
                    config.settings_error = None;
                }
                Err(error) => config.settings_error = Some(error),
            }
        }
    }
    if work.config.keys != config.taken.keys {
        config.taken.keys = work.config.keys.clone();
        if let Some(file) = &config.taken.keys {
            let now = match keymap(file, &state.picker.home) {
                Ok(keymap) => {
                    state.chrome.keymap = keymap;
                    None
                }
                Err(error) => Some(error),
            };
            let last = std::mem::replace(&mut config.keys_error, now.clone());
            effects.extend(status_line::failing(
                &mut state.chrome.status,
                last.as_deref(),
                now.as_deref(),
            ));
        }
    }
    effects
}

/// The bindings `file` holds over the defaults; the defaults alone when it
/// cannot be read.
fn keymap(file: &ConfigFile, home: &std::path::Path) -> Result<Keymap, String> {
    match &file.text {
        FileText::Read { text } => {
            Keymap::with_user(text).map_err(|e| format!("{}: {e}", tilde(&file.path, home)))
        }
        FileText::Missing | FileText::Unreadable { .. } => Ok(Keymap::defaults()),
    }
}

#[cfg(test)]
pub(crate) mod testing {
    use adroit_protocol::{
        Change, ConfigChange, ConfigFile, ConfigWork, Event, FileText, Seq, WorkPatch, WorkSnapshot,
    };

    pub(crate) fn settings_file(text: &str) -> ConfigFile {
        ConfigFile {
            path: "/c/settings.toml".into(),
            text: FileText::Read { text: text.into() },
        }
    }

    pub(crate) fn keys_file(text: &str) -> ConfigFile {
        ConfigFile {
            path: "/home/u/.config/adroit/keys.toml".into(),
            text: FileText::Read { text: text.into() },
        }
    }

    /// The work as the host first describes it, with these settings.
    pub(crate) fn configured(settings: &str) -> Event {
        Event::Snapshot(Box::new(WorkSnapshot {
            config: ConfigWork {
                settings: Some(settings_file(settings)),
                keys: None,
            },
            ..Default::default()
        }))
    }

    /// The patch numbered `seq` that changes the configuration so.
    pub(crate) fn reconfigured(seq: u64, change: ConfigChange) -> Event {
        Event::Patch(WorkPatch {
            seq: Seq(seq),
            changes: vec![Change::Config(change)],
        })
    }
}

#[cfg(test)]
mod tests {
    use adroit_protocol::{ConfigChange, ConfigFile, FileText};

    use super::testing::{configured, keys_file, reconfigured, settings_file};
    use crate::effect::Effect;
    use crate::keymap::Keymap;
    use crate::msg::Msg;
    use crate::scenario::Scenario;

    fn home() -> Msg {
        Msg::Opened {
            home: "/home/u".into(),
        }
    }

    #[test]
    fn the_settings_are_as_designed_until_the_host_reads_them() {
        let run = Scenario::new();
        assert_eq!(run.state().config.settings, Default::default());
        let run = run.event(configured("zoom = 1.25\nnotify_waiting = false\n"));
        assert_eq!(run.state().config.settings.zoom, 1.25);
        assert!(!run.state().config.settings.notify_waiting);
        assert_eq!(run.state().config.settings_error, None);
    }

    #[test]
    fn settings_that_cannot_be_had_leave_those_in_force_and_say_why_until_fixed() {
        let settings = |seq, text: FileText| {
            reconfigured(
                seq,
                ConfigChange::Settings {
                    file: ConfigFile {
                        path: "/c/settings.toml".into(),
                        text,
                    },
                },
            )
        };
        let run = Scenario::new()
            .event(configured("zoom = 1.25\n"))
            .event(settings(
                1,
                FileText::Read {
                    text: "zoom = ".into(),
                },
            ));
        assert_eq!(run.state().config.settings.zoom, 1.25);
        let error = run.state().config.settings_error.clone().unwrap();
        assert!(
            error.starts_with("Cannot load /c/settings.toml: "),
            "{error}"
        );
        let run = run.event(settings(
            2,
            FileText::Unreadable {
                error: "denied".into(),
            },
        ));
        assert_eq!(
            run.state().config.settings_error.as_deref(),
            Some("Cannot read /c/settings.toml: denied")
        );
        let run = run.event(settings(3, FileText::Missing));
        assert_eq!(run.state().config.settings, Default::default());
        assert_eq!(run.state().config.settings_error, None);
    }

    #[test]
    fn the_key_bindings_read_go_over_the_defaults_and_a_broken_file_is_told_until_it_reads() {
        let keys = |seq, text: &str| {
            reconfigured(
                seq,
                ConfigChange::Keys {
                    file: keys_file(text),
                },
            )
        };
        let user = "[global]\n\"Ctrl+h\" = \"keys.help\"\n";
        let run = Scenario::new()
            .send(home())
            .event(configured(""))
            .event(keys(1, user));
        assert_eq!(run.state().chrome.keymap, Keymap::with_user(user).unwrap());
        let run = run.event(keys(2, "[global]\n\"Ctrl+h\" = \"no.such\"\n"));
        assert_eq!(
            run.state().chrome.keymap,
            Keymap::with_user(user).unwrap(),
            "a broken file leaves the bindings in force"
        );
        let told = run.state().chrome.status.line.clone().unwrap();
        assert!(told.starts_with("~/.config/adroit/keys.toml: "), "{told}");
        let run = run.event(keys(3, ""));
        assert_eq!(run.state().chrome.keymap, Keymap::defaults());
        assert_eq!(run.state().chrome.status.line, None);
    }

    #[test]
    fn a_key_bindings_file_missing_or_unreadable_leaves_the_defaults() {
        for text in [
            FileText::Missing,
            FileText::Unreadable {
                error: "denied".into(),
            },
        ] {
            let run = Scenario::new()
                .event(configured(""))
                .event(reconfigured(
                    1,
                    ConfigChange::Keys {
                        file: keys_file("[global]\n\"Ctrl+h\" = \"keys.help\"\n"),
                    },
                ))
                .event(reconfigured(
                    2,
                    ConfigChange::Keys {
                        file: ConfigFile {
                            text: text.clone(),
                            ..keys_file("")
                        },
                    },
                ));
            assert_eq!(run.state().chrome.keymap, Keymap::defaults(), "{text:?}");
            assert_eq!(run.state().chrome.status.line, None, "{text:?}");
        }
    }

    #[test]
    fn the_same_files_read_again_change_nothing() {
        let run = Scenario::new()
            .event(configured("zoom = 1.25\n"))
            .send(Msg::Zoom(crate::zoom::Step::In))
            .event(reconfigured(
                1,
                ConfigChange::Settings {
                    file: settings_file("zoom = 1.25\n"),
                },
            ));
        assert!(
            !run.last_effects()
                .iter()
                .any(|e| matches!(e, Effect::Zoom(_)))
        );
    }
}
