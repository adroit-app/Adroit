//! Display and input preferences belonging to this computer, across environments.

use adroit_core::settings::{ConsoleSettings, Settings};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Preferences {
    zoom: f64,
    font_family: String,
    font_size: f64,
    font_weight: u16,
    prompt_min_rows: usize,
    prompt_max_rows: usize,
    notify_waiting: bool,
    console: ConsoleSettings,
}

impl Preferences {
    pub fn of(settings: &Settings) -> Self {
        Self {
            zoom: settings.zoom,
            font_family: settings.font_family.clone(),
            font_size: settings.font_size,
            font_weight: settings.font_weight,
            prompt_min_rows: settings.prompt_min_rows,
            prompt_max_rows: settings.prompt_max_rows,
            notify_waiting: settings.notify_waiting,
            console: settings.console.clone(),
        }
    }
    pub fn apply(&self, settings: &mut Settings) {
        settings.zoom = self.zoom;
        settings.font_family.clone_from(&self.font_family);
        settings.font_size = self.font_size;
        settings.font_weight = self.font_weight;
        settings.prompt_min_rows = self.prompt_min_rows;
        settings.prompt_max_rows = self.prompt_max_rows;
        settings.notify_waiting = self.notify_waiting;
        settings.console = self.console.clone();
    }
}

fn path() -> std::path::PathBuf {
    crate::paths::config_dir().join("window.toml")
}

pub(crate) fn load() -> Result<Preferences, String> {
    let path = path();
    match adroit_connect::read_config(&path, 65536).map_err(|e| e.to_string())? {
        Some(text) => toml::from_str(&text).map_err(|e| format!("{}: {e}", path.display())),
        None => {
            let legacy = crate::paths::config_dir().join("settings.toml");
            let text = adroit_connect::read_config(&legacy, 1024 * 1024)
                .map_err(|e| e.to_string())?
                .map_or(adroit_protocol::FileText::Missing, |text| {
                    adroit_protocol::FileText::Read { text }
                });
            Settings::read(&adroit_protocol::ConfigFile { path: legacy, text })
                .map(|s| Preferences::of(&s))
        }
    }
}

pub(crate) fn save(settings: &Settings) -> Result<(), String> {
    load()?;
    adroit_connect::environments::write(&path(), &Preferences::of(&settings.clone().sane()))
}

pub(crate) fn apply(settings: &mut Settings) {
    if adroit_connect::environments::active()
        .ok()
        .flatten()
        .is_some()
        && let Ok(preferences) = load()
    {
        preferences.apply(settings);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn client_preferences_do_not_change_agents_or_working_paths() {
        let first = Settings {
            font_family: "Client font".into(),
            zoom: 1.25,
            ..Settings::default()
        };
        let preferences = Preferences::of(&first);
        let mut work = Settings {
            scratch_dir: "/remote/work".into(),
            ..Settings::default()
        };
        let agents = work.profiles.clone();
        preferences.apply(&mut work);
        assert_eq!(work.zoom, 1.25);
        assert_eq!(work.font_family, "Client font");
        assert_eq!(work.scratch_dir, "/remote/work");
        assert_eq!(work.profiles, agents);
    }
}
