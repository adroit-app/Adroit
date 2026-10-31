//! The user's configuration as the window works by it: the settings and the
//! key bindings, taken in from the files as the host last read them.

use adroit_model::paths::tilde;
use adroit_protocol::{Colours, ConfigFile, ConfigWork, Configured, FileText};

use crate::chrome::status_line::{self, StatusLine};
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
    /// Where scratch sessions run, as the host says.
    pub scratch: std::path::PathBuf,
    /// Why the key bindings file cannot be had, while it cannot.
    keys_error: Option<String>,
    /// The files as they were last taken in.
    taken: ConfigWork,
}

impl Config {
    /// The configuration of the files `work` holds, as a host of another
    /// environment read them: its settings, or the settings as designed and
    /// why they cannot be had.
    pub fn of_work(work: &ConfigWork) -> Self {
        let mut config = Config {
            taken: work.clone(),
            ..Config::default()
        };
        if let Some(file) = &work.settings {
            match read_settings(file, work.configured.as_ref()) {
                Ok(settings) => config.settings = settings,
                Err(error) => config.settings_error = Some(error),
            }
        }
        if let Some(made) = &work.configured {
            config.scratch = made.scratch.clone();
        }
        config
    }

    /// The files as they were last taken in, and what the host made of
    /// them.
    pub fn files(&self) -> &ConfigWork {
        &self.taken
    }
}

/// The files the host read, taken in once each time one changes: the
/// settings replace those in force, and the key bindings the chrome's. A
/// file that cannot be had leaves what is in force and says why, the key
/// bindings' in the status line until they read again.
pub(crate) fn take_in(state: &mut ViewState) -> Vec<Effect> {
    let Some(work) = state.hosts.own().work.as_ref() else {
        return Vec::new();
    };
    let config = &mut state.config;
    let mut effects = Vec::new();
    // The one theme an earlier version named waits for the host's list to
    // tell its kind.
    let listed = config.settings.theme.is_some()
        && work.config.configured.is_some()
        && work.config.configured != config.taken.configured;
    if work.config.settings != config.taken.settings || listed {
        config.taken.settings = work.config.settings.clone();
        if let Some(file) = &config.taken.settings {
            match read_settings(file, work.config.configured.as_ref()) {
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
    if work.config.configured != config.taken.configured {
        let was = std::mem::replace(&mut config.taken.configured, work.config.configured.clone());
        if let Some(made) = &config.taken.configured {
            config.scratch = made.scratch.clone();
            if was.as_ref().map(|c| &c.theme) != Some(&made.theme) {
                effects.extend(theme_taken(
                    &mut state.chrome.status,
                    &made.theme.colours,
                    was.is_none(),
                ));
            }
        }
    }
    effects
}

/// The settings `file` holds, the one theme an earlier version named
/// migrated by the kind the host `made` it out to be.
fn read_settings(file: &ConfigFile, made: Option<&Configured>) -> Result<Settings, String> {
    let settings = Settings::read(file)?;
    Ok(match (&settings.theme, made) {
        (Some(theme), Some(made)) => {
            let kind = made.kind_of(theme);
            settings.migrated(kind)
        }
        _ => settings,
    })
}

/// The consoles are drawn in `colours`; ones that cannot be had leave them
/// as they are and say why, but for the first, which leaves adroit's own.
fn theme_taken(status: &mut StatusLine, colours: &Colours, first: bool) -> Vec<Effect> {
    match colours {
        Colours::Unreadable { .. } if first => vec![Effect::Palette(Colours::Own)],
        Colours::Unreadable { error } => status_line::set(status, Some(error.clone())),
        colours => vec![Effect::Palette(colours.clone())],
    }
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
    use adroit_protocol::{ConfigChange, ConfigFile, Event, FileText};

    use crate::testing::{patch, snapshot};

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

    /// The work as the host first describes it, with these settings, before
    /// any change.
    pub(crate) fn configured(settings: &str) -> Event {
        snapshot()
            .seq(0)
            .with(|w| w.config.settings = Some(settings_file(settings)))
            .into()
    }

    /// The patch numbered `seq` that changes the configuration so.
    pub(crate) fn reconfigured(seq: u64, change: ConfigChange) -> Event {
        patch(seq).change(change).into()
    }
}

#[cfg(test)]
mod tests {
    use crate::testing::here;
    use adroit_protocol::{Colours, ConfigChange, ConfigFile, Configured, FileText, ThemeRead};

    use super::testing::{configured, keys_file, settings_file};
    use crate::effect::Effect;
    use crate::keymap::Keymap;
    use crate::msg::Msg;
    use crate::scenario::Scenario;

    fn home() -> Msg {
        Msg::Opened {
            environment: here(),
            home: "/home/u".into(),
        }
    }

    #[test]
    fn an_environment_s_configuration_is_read_from_the_files_its_own_host_read() {
        use adroit_protocol::ConfigWork;

        let work = ConfigWork {
            settings: Some(settings_file("zoom = 1.5\n")),
            ..ConfigWork::default()
        };
        let config = super::Config::of_work(&work);
        assert_eq!(config.settings.zoom, 1.5);
        assert_eq!(config.files(), &work);
        let unreadable = ConfigWork {
            settings: Some(settings_file("zoom = ")),
            ..ConfigWork::default()
        };
        let config = super::Config::of_work(&unreadable);
        assert_eq!(config.settings, Default::default());
        assert!(config.settings_error.is_some());
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
        let settings = |text: FileText| ConfigChange::Settings {
            file: ConfigFile {
                path: "/c/settings.toml".into(),
                text,
            },
        };
        let run = Scenario::new()
            .event(configured("zoom = 1.25\n"))
            .tell(settings(FileText::Read {
                text: "zoom = ".into(),
            }));
        assert_eq!(run.state().config.settings.zoom, 1.25);
        let error = run.state().config.settings_error.clone().unwrap();
        assert!(
            error.starts_with("Cannot load /c/settings.toml: "),
            "{error}"
        );
        let run = run.tell(settings(FileText::Unreadable {
            error: "denied".into(),
        }));
        assert_eq!(
            run.state().config.settings_error.as_deref(),
            Some("Cannot read /c/settings.toml: denied")
        );
        let run = run.tell(settings(FileText::Missing));
        assert_eq!(run.state().config.settings, Default::default());
        assert_eq!(run.state().config.settings_error, None);
    }

    #[test]
    fn the_key_bindings_read_go_over_the_defaults_and_a_broken_file_is_told_until_it_reads() {
        let keys = |text: &str| ConfigChange::Keys {
            file: keys_file(text),
        };
        let user = "[global]\n\"Ctrl+h\" = \"keys.help\"\n";
        let run = Scenario::new()
            .send(home())
            .event(configured(""))
            .tell(keys(user));
        assert_eq!(run.state().chrome.keymap, Keymap::with_user(user).unwrap());
        let run = run.tell(keys("[global]\n\"Ctrl+h\" = \"no.such\"\n"));
        assert_eq!(
            run.state().chrome.keymap,
            Keymap::with_user(user).unwrap(),
            "a broken file leaves the bindings in force"
        );
        let told = run.state().chrome.status.line.clone().unwrap();
        assert!(told.starts_with("~/.config/adroit/keys.toml: "), "{told}");
        let run = run.tell(keys(""));
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
                .tell(ConfigChange::Keys {
                    file: keys_file("[global]\n\"Ctrl+h\" = \"keys.help\"\n"),
                })
                .tell(ConfigChange::Keys {
                    file: ConfigFile {
                        text: text.clone(),
                        ..keys_file("")
                    },
                });
            assert_eq!(run.state().chrome.keymap, Keymap::defaults(), "{text:?}");
            assert_eq!(run.state().chrome.status.line, None, "{text:?}");
        }
    }

    #[test]
    fn the_same_files_read_again_change_nothing() {
        let run = Scenario::new()
            .event(configured("zoom = 1.25\n"))
            .send(Msg::Zoom(crate::zoom::Step::In))
            .tell(ConfigChange::Settings {
                file: settings_file("zoom = 1.25\n"),
            });
        assert!(
            !run.last_effects()
                .iter()
                .any(|e| matches!(e, Effect::Zoom(_)))
        );
    }

    fn made(scratch: &str, theme: &str, colours: Colours) -> ConfigChange {
        ConfigChange::Configured {
            configured: Configured {
                scratch: scratch.into(),
                theme: ThemeRead {
                    name: theme.into(),
                    colours,
                },
                ..Default::default()
            },
        }
    }

    fn palettes(run: &Scenario) -> Vec<Colours> {
        run.effects()
            .iter()
            .filter_map(|e| match e {
                Effect::Palette(c) => Some(c.clone()),
                _ => None,
            })
            .collect()
    }

    #[test]
    fn the_host_s_theme_colours_the_consoles_once_each_time_another_is_read() {
        let nord = || Colours::Theme {
            text: "background = #2e3440\n".into(),
        };
        let run = Scenario::new()
            .event(configured(""))
            .tell(made("/s", "Nord", nord()))
            .tell(made("/t", "Nord", nord()))
            .tell(made("/t", "Adroit", Colours::Own));
        assert_eq!(palettes(&run), [nord(), Colours::Own]);
        assert_eq!(run.state().config.scratch, std::path::PathBuf::from("/t"));
    }

    #[test]
    fn a_theme_that_cannot_be_had_says_why_but_the_first_leaves_adroit_s_own() {
        let cannot = |why: &str| Colours::Unreadable { error: why.into() };
        let run = Scenario::new().event(configured("")).tell(made(
            "/s",
            "Gone",
            cannot("no theme named \"Gone\""),
        ));
        assert_eq!(palettes(&run), [Colours::Own]);
        assert_eq!(run.state().chrome.status.line, None);
        let run = run.tell(made("/s", "Lost", cannot("no theme named \"Lost\"")));
        assert_eq!(palettes(&run), [Colours::Own]);
        assert_eq!(
            run.state().chrome.status.line.as_deref(),
            Some("no theme named \"Lost\"")
        );
    }

    #[test]
    fn the_one_theme_an_earlier_version_named_is_taken_by_its_kind_once_the_host_lists_it() {
        use crate::settings::{Appearance, ThemeKind};
        use adroit_protocol::ThemeListed;
        let listed = ConfigChange::Configured {
            configured: Configured {
                themes: vec![ThemeListed {
                    name: "Latte".into(),
                    kind: ThemeKind::Light,
                }],
                ..Default::default()
            },
        };
        let run = Scenario::new().event(configured("theme = \"Latte\"\n"));
        assert_eq!(run.state().config.settings.theme.as_deref(), Some("Latte"));
        let run = run.tell(listed);
        let settings = &run.state().config.settings;
        assert_eq!(settings.theme, None);
        assert_eq!(settings.appearance, Appearance::Light);
        assert_eq!(settings.light_theme, "Latte");
    }
}
